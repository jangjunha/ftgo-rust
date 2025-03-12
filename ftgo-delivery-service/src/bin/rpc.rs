use chrono::Utc;
use diesel::dsl::{insert_into, update};
use diesel::prelude::*;
use diesel::result::Error::NotFound;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_delivery_service::events::DeliveryEventPublisher;
use ftgo_proto::delivery_service::delivery_service_server::{
    DeliveryService, DeliveryServiceServer,
};
use ftgo_proto::delivery_service::{
    ActionInfo, Courier, CourierAction, CourierPlan, DeliveryActionType, DeliveryInfo,
    DeliveryState, DeliveryStatus, DropoffDeliveryPayload, GetCourierPayload,
    GetDeliveryStatusPayload, PickupDeliveryPayload, UpdateCourierAvailabilityPayload,
};
use ftgo_proto::kitchen_service::GetTicketPayload;
use prost_types::Timestamp;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use ftgo_delivery_service::{establish_connection, get_kitchen_client, models};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Default)]
pub struct DeliveryServiceImpl {}

#[tonic::async_trait]
impl DeliveryService for DeliveryServiceImpl {
    async fn update_courier_availability(
        &self,
        request: Request<UpdateCourierAvailabilityPayload>,
    ) -> Result<Response<()>, Status> {
        use ftgo_delivery_service::schema::couriers::dsl::*;

        let payload = request.into_inner();
        let cid = payload
            .courier_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid courier id"))?;

        let conn = &mut establish_connection();
        let courier = match couriers
            .select(models::Courier::as_select())
            .find(&cid)
            .first::<models::Courier>(conn)
        {
            Ok(courier) => courier,
            Err(NotFound) => return Err(Status::not_found("Courier not found")),
            Err(_) => return Err(Status::internal("Cannot fetch courier")),
        };

        update(couriers)
            .set(available.eq(payload.available))
            .filter(id.eq(courier.id))
            .execute(conn)
            .map_err(|_| Status::internal("Cannot update availability"))?;

        Ok(Response::new(()))
    }

    async fn get_delivery_status(
        &self,
        request: Request<GetDeliveryStatusPayload>,
    ) -> Result<Response<DeliveryStatus>, Status> {
        use ftgo_delivery_service::schema::deliveries::dsl::*;

        let payload = request.into_inner();
        let did = payload
            .delivery_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid delivery id"))?;

        let conn = &mut establish_connection();

        let delivery = match deliveries
            .select(models::Delivery::as_select())
            .find(&did)
            .first::<models::Delivery>(conn)
        {
            Ok(delivery) => delivery,
            Err(NotFound) => return Err(Status::not_found("Delivery not found")),
            Err(_) => return Err(Status::internal("Cannot fetch delivery")),
        };

        let courier_actions = match delivery.assigned_courier_id {
            Some(cid) => {
                use ftgo_delivery_service::schema::courier_actions::dsl::*;
                courier_actions
                    .select(models::CourierAction::as_select())
                    .filter(courier_id.eq(cid))
                    .filter(delivery_id.eq(delivery.id))
                    .load::<models::CourierAction>(conn)
                    .map_err(|_| Status::internal("Cannot fetch actions"))?
            }
            None => vec![],
        };

        Ok(Response::new(DeliveryStatus {
            delivery_info: Some(DeliveryInfo {
                id: delivery.id.to_string(),
                state: DeliveryState::from(delivery.state).into(),
                pickup_time: delivery.pickup_time.map(|time| Timestamp {
                    seconds: time.timestamp(),
                    nanos: time.timestamp_subsec_nanos() as i32,
                }),
                delivery_time: delivery.delivery_time.map(|time| Timestamp {
                    seconds: time.timestamp(),
                    nanos: time.timestamp_subsec_nanos() as i32,
                }),
            }),
            assigned_courier_id: delivery
                .assigned_courier_id
                .ok_or(Status::internal("Courier not assigned"))?
                .to_string(),
            courier_actions: courier_actions
                .into_iter()
                .map(|a| ActionInfo {
                    r#type: DeliveryActionType::from(a.type_).into(),
                })
                .collect(),
        }))
    }

    async fn pickup_delivery(
        &self,
        request: Request<PickupDeliveryPayload>,
    ) -> Result<Response<()>, Status> {
        use ftgo_delivery_service::schema::deliveries::dsl::*;

        let payload = request.into_inner();
        let did = payload
            .delivery_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid delivery id"))?;
        let now = Utc::now();

        enum Error {
            NotFound,
            FailedPrecondition,
            AlreadyPerformed,
            Unexpected,
        }

        impl From<diesel::result::Error> for Error {
            fn from(_: diesel::result::Error) -> Self {
                Error::Unexpected
            }
        }

        let mut kitchen_service = get_kitchen_client()
            .await
            .map_err(|_| Status::internal("Cannot connect to kitchen service"))?;
        let ticket = kitchen_service
            .get_ticket(Request::new(GetTicketPayload {
                ticket_id: did.to_string(),
            }))
            .await
            .map_err(|_| Status::internal("Cannot retrieve ticket status"))?
            .into_inner();

        let conn = &mut establish_connection();
        let _ = conn
            .transaction::<_, Error, _>(|conn| {
                let delivery = match deliveries
                    .select(models::Delivery::as_select())
                    .find(&did)
                    .first::<models::Delivery>(conn)
                {
                    Ok(delivery) => delivery,
                    Err(NotFound) => return Err(Error::NotFound),
                    Err(_) => return Err(Error::Unexpected),
                };

                if delivery.pickup_time.is_none() {
                    return Err(Error::AlreadyPerformed);
                }
                if ftgo_proto::kitchen_service::TicketState::try_from(ticket.state).unwrap()
                    != ftgo_proto::kitchen_service::TicketState::ReadyForPickup
                {
                    return Err(Error::FailedPrecondition);
                }

                update(deliveries)
                    .set(pickup_time.eq(now))
                    .filter(id.eq(delivery.id))
                    .execute(conn)
                    .map_err(|_| Error::Unexpected)?;

                let mut publisher = DeliveryEventPublisher::new(conn);
                publisher.delivery_picked_up(&delivery);

                Ok(())
            })
            .map_err(|err| match err {
                Error::NotFound => Status::not_found("Delivery not found"),
                Error::AlreadyPerformed => Status::failed_precondition("Action already performed"),
                Error::FailedPrecondition => {
                    Status::failed_precondition("Unsupported delivery state")
                }
                _ => Status::internal("Error while loading"),
            })?;

        Ok(Response::new(()))
    }

    async fn dropoff_delivery(
        &self,
        request: Request<DropoffDeliveryPayload>,
    ) -> Result<Response<()>, Status> {
        use ftgo_delivery_service::schema::deliveries::dsl::*;

        let payload = request.into_inner();
        let did = payload
            .delivery_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid delivery id"))?;
        let now = Utc::now();

        enum Error {
            NotFound,
            AlreadyPerformed,
            Unexpected,
        }

        impl From<diesel::result::Error> for Error {
            fn from(_: diesel::result::Error) -> Self {
                Error::Unexpected
            }
        }

        let conn = &mut establish_connection();
        let _ = conn
            .transaction::<_, Error, _>(|conn| {
                let delivery = match deliveries
                    .select(models::Delivery::as_select())
                    .find(&did)
                    .first::<models::Delivery>(conn)
                {
                    Ok(delivery) => delivery,
                    Err(NotFound) => return Err(Error::NotFound),
                    Err(_) => return Err(Error::Unexpected),
                };

                if delivery.delivery_time.is_some() {
                    return Err(Error::AlreadyPerformed);
                }

                update(deliveries)
                    .set(delivery_time.eq(now))
                    .filter(id.eq(delivery.id))
                    .execute(conn)
                    .map_err(|_| Error::Unexpected)?;

                let mut publisher = DeliveryEventPublisher::new(conn);
                publisher.delivery_dropoff(&delivery);

                Ok(())
            })
            .map_err(|err| match err {
                Error::NotFound => Status::not_found("Delivery not found"),
                Error::AlreadyPerformed => Status::failed_precondition("Action already performed"),
                _ => Status::internal("Error while loading"),
            })?;

        Ok(Response::new(()))
    }

    async fn create_conrier(&self, _: Request<()>) -> Result<Response<Courier>, Status> {
        use ftgo_delivery_service::schema::couriers::dsl::*;

        let courier = models::Courier {
            id: Uuid::new_v4(),
            available: false,
        };

        let conn = &mut establish_connection();
        insert_into(couriers)
            .values(&courier)
            .execute(conn)
            .map_err(|_| Status::internal("Cannot insert courier"))?;

        Ok(Response::new(Courier {
            id: courier.id.to_string(),
            available: courier.available,
        }))
    }

    async fn get_courier(
        &self,
        request: Request<GetCourierPayload>,
    ) -> Result<Response<Courier>, Status> {
        use ftgo_delivery_service::schema::couriers::dsl::*;

        let payload = request.into_inner();
        let cid = payload
            .courier_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid courier id"))?;

        let conn = &mut establish_connection();
        let courier = couriers
            .select(models::Courier::as_select())
            .find(&cid)
            .first::<models::Courier>(conn)
            .map_err(|_| Status::not_found("Courier not found"))?;

        Ok(Response::new(Courier {
            id: courier.id.to_string(),
            available: courier.available,
        }))
    }

    async fn get_courier_plan(
        &self,
        request: Request<GetCourierPayload>,
    ) -> Result<Response<CourierPlan>, Status> {
        use ftgo_delivery_service::schema::courier_actions::dsl::*;
        use ftgo_delivery_service::schema::couriers::dsl::*;

        let payload = request.into_inner();
        let cid = payload
            .courier_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid courier id"))?;

        let conn = &mut establish_connection();
        let courier = couriers
            .select(models::Courier::as_select())
            .find(&cid)
            .first::<models::Courier>(conn)
            .map_err(|_| Status::not_found("Courier not found"))?;

        let actions = models::CourierAction::belonging_to(&courier)
            .select(models::CourierAction::as_select())
            .order(time.desc())
            .get_results(conn)
            .map_err(|_| Status::internal("Cannot fetch actions"))?;

        Ok(Response::new(CourierPlan {
            actions: actions
                .into_iter()
                .map(|a| CourierAction {
                    r#type: DeliveryActionType::from(a.type_).into(),
                    delivery_id: a.delivery_id.to_string(),
                    address: a.address.to_string(),
                    time: Some(Timestamp {
                        seconds: a.time.timestamp(),
                        nanos: a.time.timestamp_subsec_nanos() as i32,
                    }),
                })
                .collect(),
        }))
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection();
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    let addr = "0.0.0.0:8108".parse().unwrap();
    let delivery_service = DeliveryServiceImpl::default();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<DeliveryServiceServer<DeliveryServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(DeliveryServiceServer::new(delivery_service))
        .serve(addr)
        .await?;

    Ok(())
}
