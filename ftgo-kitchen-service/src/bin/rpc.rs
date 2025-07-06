use chrono::Utc;
use diesel::dsl::update;
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_kitchen_service::events::KitchenEventPublisher;
use ftgo_proto::kitchen_service::{
    AcceptTicketPayload, GetTicketPayload, ListTicketPayload, ListTicketResponse,
    PreparingTicketPayload, ReadyForPickupTicketPayload, Ticket, TicketEdge, TicketLineItem,
};
use prost_types::Timestamp;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use ftgo_proto::kitchen_service::kitchen_service_server::{KitchenService, KitchenServiceServer};

use ftgo_kitchen_service::{establish_connection, models, schema};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Default)]
pub struct KitchenServiceImpl {}

#[tonic::async_trait]
impl KitchenService for KitchenServiceImpl {
    async fn list_tickets(
        &self,
        request: Request<ListTicketPayload>,
    ) -> Result<Response<ListTicketResponse>, Status> {
        use schema::tickets::dsl::*;

        let payload = request.into_inner();
        let rid = payload
            .restaurant_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid restaurant_id"))?;
        let after = payload
            .after
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| Status::invalid_argument("Invalid after"))?;
        let before = payload
            .before
            .map(|s| s.parse::<usize>())
            .transpose()
            .map_err(|_| Status::invalid_argument("Invalid before"))?;
        let first = payload.first;
        let last = payload.last;

        let base_query = tickets
            .select(models::Ticket::as_select())
            .filter(restaurant_id.eq(rid));
        let query = match (after, before, first, last) {
            (None, None, Some(first), None) => base_query
                .order_by(sequence.asc())
                .limit(first.into())
                .into_boxed(),
            (None, None, None, Some(last)) => base_query
                .order_by(sequence.desc())
                .limit(last.into())
                .into_boxed(),
            (Some(after), None, Some(first), None) => base_query
                .filter(sequence.lt(after as i64))
                .order_by(sequence.asc())
                .limit(first.into())
                .into_boxed(),
            (None, Some(before), None, Some(last)) => base_query
                .filter(sequence.gt(before as i64))
                .order_by(sequence.desc())
                .limit(last.into())
                .into_boxed(),
            (Some(_), Some(_), _, _) => {
                return Err(Status::invalid_argument(
                    "Only one of `after` or `before` can be given.",
                ))
            }
            (_, _, Some(_), Some(_)) => {
                return Err(Status::invalid_argument(
                    "Only one of `first` or `last` can be given.",
                ))
            }
            (_, _, None, None) => {
                return Err(Status::invalid_argument(
                    "One of `first` or `last` must be given.",
                ))
            }
            (Some(_), _, None, _) => {
                return Err(Status::invalid_argument(
                    "`first` required if `after` is given.",
                ))
            }
            (_, Some(_), _, None) => {
                return Err(Status::invalid_argument(
                    "`last` required if `before` is given.",
                ))
            }
        };

        let conn = &mut establish_connection();
        let results: Vec<models::Ticket> = query.load(conn).expect("Error loading tickets");

        let line_items = models::TicketLineItem::belonging_to(&results)
            .select(models::TicketLineItem::as_select())
            .load(conn)
            .expect("Error loading ticket_line_items")
            .grouped_by(&results);

        Ok(Response::new(ListTicketResponse {
            edges: results
                .into_iter()
                .zip(line_items)
                .map(|(r, line_items)| {
                    Ok::<TicketEdge, Status>(TicketEdge {
                        node: Some(serialize_ticket(&r, &line_items)),
                        cursor: r.sequence.to_string(),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        }))
    }

    async fn get_ticket(
        &self,
        request: Request<GetTicketPayload>,
    ) -> Result<Response<Ticket>, Status> {
        use schema::tickets::dsl::*;

        let payload = request.into_inner();
        let tid = payload
            .ticket_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid ticket_id"))?;

        let conn = &mut establish_connection();
        let result: models::Ticket = match tickets
            .find(&tid)
            .select(models::Ticket::as_select())
            .first(conn)
        {
            Ok(ticket) => ticket,
            Err(NotFound) => return Err(Status::not_found("Ticket not found")),
            Err(_) => return Err(Status::internal("Error loading ticket")),
        };
        let line_items = models::TicketLineItem::belonging_to(&result)
            .select(models::TicketLineItem::as_select())
            .load(conn)
            .expect("Error loading ticket_line_items");

        Ok(Response::new(serialize_ticket(&result, &line_items)))
    }

    async fn accept_ticket(
        &self,
        request: Request<AcceptTicketPayload>,
    ) -> Result<Response<()>, Status> {
        use schema::tickets::dsl::*;

        let payload = request.into_inner();
        let tid = payload
            .ticket_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid ticket_id"))?;
        let now = Utc::now();

        enum Error {
            NotFound,
            UnsupportedStateTransition,
            Unexpected,
        }

        impl From<diesel::result::Error> for Error {
            fn from(_: diesel::result::Error) -> Self {
                Error::Unexpected
            }
        }

        let conn = &mut establish_connection();
        let _ = conn
            .transaction(|conn| {
                let result: models::Ticket = tickets
                    .find(&tid)
                    .select(models::Ticket::as_select())
                    .first(conn)
                    .map_err(|err| match err {
                        diesel::result::Error::NotFound => Error::NotFound,
                        _ => Error::Unexpected,
                    })?;

                if result.state != ftgo_kitchen_service::models::TicketState::AwaitingAcceptance {
                    return Err(Error::UnsupportedStateTransition);
                }

                update(tickets)
                    .set((
                        state.eq(ftgo_kitchen_service::models::TicketState::Accepted),
                        previous_state.eq(Some(result.state)),
                        ready_by.eq(now),
                    ))
                    .filter(id.eq(tid))
                    .execute(conn)?;

                let mut publisher = KitchenEventPublisher::new(conn);
                publisher.ticket_accepted(&result);

                Ok(())
            })
            .map_err(|err| match err {
                Error::NotFound => Status::not_found("Ticket not found"),
                Error::UnsupportedStateTransition => {
                    Status::failed_precondition("Unsupported state transition")
                }
                _ => Status::internal("Error loading ticket"),
            })?;

        Ok(Response::new(()))
    }

    async fn preparing_ticket(
        &self,
        request: Request<PreparingTicketPayload>,
    ) -> Result<Response<Ticket>, Status> {
        use schema::tickets::dsl::*;

        let payload = request.into_inner();
        let tid = payload
            .ticket_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid ticket_id"))?;
        let now = Utc::now();

        enum Error {
            NotFound,
            UnsupportedStateTransition,
            Unexpected,
        }

        impl From<diesel::result::Error> for Error {
            fn from(_: diesel::result::Error) -> Self {
                Error::Unexpected
            }
        }

        let conn = &mut establish_connection();
        let (ticket, line_items) = conn
            .transaction(|conn| {
                let result: models::Ticket = tickets
                    .find(&tid)
                    .select(models::Ticket::as_select())
                    .first(conn)
                    .map_err(|err| match err {
                        diesel::result::Error::NotFound => Error::NotFound,
                        _ => Error::Unexpected,
                    })?;
                let line_items = models::TicketLineItem::belonging_to(&result)
                    .select(models::TicketLineItem::as_select())
                    .load(conn)
                    .map_err(|_| Error::Unexpected)?;

                if result.state != ftgo_kitchen_service::models::TicketState::Accepted {
                    return Err(Error::UnsupportedStateTransition);
                }

                update(tickets)
                    .set((
                        state.eq(ftgo_kitchen_service::models::TicketState::Preparing),
                        previous_state.eq(Some(result.state)),
                        preparing_time.eq(now),
                    ))
                    .filter(id.eq(tid))
                    .execute(conn)?;

                let mut publisher = KitchenEventPublisher::new(conn);
                publisher.ticket_preparing_started(&result);

                Ok((result, line_items))
            })
            .map_err(|err| match err {
                Error::NotFound => Status::not_found("Ticket not found"),
                Error::UnsupportedStateTransition => {
                    Status::failed_precondition("Unsupported state transition")
                }
                _ => Status::internal("Error loading ticket"),
            })?;

        Ok(Response::new(serialize_ticket(&ticket, &line_items)))
    }

    async fn ready_for_pickup_ticket(
        &self,
        request: Request<ReadyForPickupTicketPayload>,
    ) -> Result<Response<Ticket>, Status> {
        use schema::tickets::dsl::*;

        let payload = request.into_inner();
        let tid = payload
            .ticket_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid ticket_id"))?;
        let now = Utc::now();

        enum Error {
            NotFound,
            UnsupportedStateTransition,
            Unexpected,
        }

        impl From<diesel::result::Error> for Error {
            fn from(_: diesel::result::Error) -> Self {
                Error::Unexpected
            }
        }

        let conn = &mut establish_connection();
        let (ticket, line_items) = conn
            .transaction(|conn| {
                let result: models::Ticket = tickets
                    .find(&tid)
                    .select(models::Ticket::as_select())
                    .first(conn)
                    .map_err(|err| match err {
                        diesel::result::Error::NotFound => Error::NotFound,
                        _ => Error::Unexpected,
                    })?;
                let line_items = models::TicketLineItem::belonging_to(&result)
                    .select(models::TicketLineItem::as_select())
                    .load(conn)
                    .map_err(|_| Error::Unexpected)?;

                if result.state != ftgo_kitchen_service::models::TicketState::Preparing {
                    return Err(Error::UnsupportedStateTransition);
                }

                update(tickets)
                    .set((
                        state.eq(ftgo_kitchen_service::models::TicketState::ReadyForPickup),
                        previous_state.eq(Some(result.state)),
                        ready_for_pickup_time.eq(now),
                    ))
                    .filter(id.eq(tid))
                    .execute(conn)?;

                let mut publisher = KitchenEventPublisher::new(conn);
                publisher.ticket_preparing_completed(&result);

                Ok((result, line_items))
            })
            .map_err(|err| match err {
                Error::NotFound => Status::not_found("Ticket not found"),
                Error::UnsupportedStateTransition => {
                    Status::failed_precondition("Unsupported state transition")
                }
                _ => Status::internal("Error loading ticket"),
            })?;

        Ok(Response::new(serialize_ticket(&ticket, &line_items)))
    }
}

fn serialize_ticket(ticket: &models::Ticket, line_items: &Vec<models::TicketLineItem>) -> Ticket {
    Ticket {
        id: ticket.id.to_string(),
        state: ftgo_proto::kitchen_service::TicketState::from(ticket.state).into(),
        sequence: Some(ticket.sequence as u32),
        restaurant_id: ticket.restaurant_id.to_string(),
        line_items: line_items
            .into_iter()
            .map(|i| TicketLineItem {
                quantity: i.quantity,
                menu_item_id: i.menu_item_id.clone(),
                name: i.name.clone(),
            })
            .collect(),
        ready_by: ticket.ready_by.map(|t| Timestamp {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos() as i32,
        }),
        accept_time: ticket.accept_time.map(|t| Timestamp {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos() as i32,
        }),
        preparing_time: ticket.preparing_time.map(|t| Timestamp {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos() as i32,
        }),
        picked_up_time: ticket.picked_up_time.map(|t| Timestamp {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos() as i32,
        }),
        ready_for_pickup_time: ticket.ready_for_pickup_time.map(|t| Timestamp {
            seconds: t.timestamp(),
            nanos: t.timestamp_subsec_nanos() as i32,
        }),
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection();
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    let addr = "0.0.0.0:8105".parse().unwrap();
    let restaurant_service = KitchenServiceImpl::default();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<KitchenServiceServer<KitchenServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(KitchenServiceServer::new(restaurant_service))
        .serve(addr)
        .await?;

    Ok(())
}
