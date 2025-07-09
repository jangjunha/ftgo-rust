use std::{collections::HashMap, env, thread::sleep, time::Duration};

use diesel::{
    delete,
    dsl::{exists, insert_into},
    query_dsl::methods::{FilterDsl, FindDsl, OrderDsl, SelectDsl},
    result::Error::NotFound,
    select, update, BelongingToDsl, Connection, ExpressionMethods, PgConnection, RunQueryDsl,
    SelectableHelper,
};
use dotenvy::dotenv;
use ftgo_kitchen_service::{
    establish_connection,
    events::KitchenEventPublisher,
    models::{self, NewOutbox},
    schema, COMMAND_CHANNEL,
};
use ftgo_proto::{
    common::CommandReply,
    kitchen_service::{CreateTicketCommandReply, KitchenCommand},
    restaurant_service::{restaurant_event, RestaurantEvent},
};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const GROUP: &'static str = "kitchen-service";

const RESTAURANT_EVENT_CHANNEL: &'static str = "restaurant.event";

enum AcceptedMessage {
    KitchenCommand(KitchenCommand),
    RestaurantEvent(RestaurantEvent),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            COMMAND_CHANNEL => Some(AcceptedMessage::KitchenCommand(
                KitchenCommand::decode(value).expect("Cannot decode kitchen command"),
            )),
            RESTAURANT_EVENT_CHANNEL => Some(AcceptedMessage::RestaurantEvent(
                RestaurantEvent::decode(value).expect("Cannot decode restaurant event"),
            )),
            _ => None,
        }
    }

    fn reply(
        conn: &mut PgConnection,
        channel: &Option<String>,
        state: &HashMap<String, String>,
        succeed: bool,
        body: Option<Vec<u8>>,
    ) -> Result<(), diesel::result::Error> {
        if let Some(channel) = channel {
            use schema::outbox::dsl::*;

            let container = CommandReply {
                state: state.clone(),
                succeed: succeed,
                body: body,
            };
            let mut buf = Vec::new();
            container.encode(&mut buf).unwrap();

            insert_into(outbox)
                .values(NewOutbox {
                    topic: channel.to_string(),
                    key: "".to_string(),
                    value: buf,
                })
                .execute(conn)?;
        }

        Ok(())
    }

    fn process(self, conn: &mut PgConnection) -> Result<(), ()> {
        match self {
            AcceptedMessage::KitchenCommand(kitchen_command) => {
                match kitchen_command.command.unwrap() {
                    ftgo_proto::kitchen_service::kitchen_command::Command::CreateTicket(
                        command,
                    ) => {
                        use schema::restaurants::dsl::*;
                        use schema::tickets::dsl::*;

                        let rid = command
                            .restaurant_id
                            .parse()
                            .expect("Cannot decode restaurant_id");

                        let _ = conn
                            .transaction::<_, diesel::result::Error, _>(|conn| {
                                let restaurant_exists = select(exists(
                                    restaurants.filter(schema::restaurants::id.eq(rid)),
                                ))
                                .get_result::<bool>(conn)?;
                                if !restaurant_exists {
                                    Self::reply(
                                        conn,
                                        &kitchen_command.reply_channel,
                                        &kitchen_command.state,
                                        false,
                                        None,
                                    )?;
                                    return Ok(());
                                }

                                let next_sequence = tickets
                                    .select(sequence)
                                    .filter(restaurant_id.eq(rid))
                                    .order(sequence.desc())
                                    .first::<i64>(conn)
                                    .or_else(|err| match err {
                                        NotFound => Ok(0),
                                        _ => Err(err),
                                    })?
                                    + 1;

                                let ticket = models::Ticket {
                                    id: command.id.parse().expect("Cannot decode ticket id"),
                                    state: models::TicketState::CreatePending,
                                    previous_state: None,
                                    restaurant_id: rid,
                                    sequence: next_sequence,
                                    ready_by: None,
                                    accept_time: None,
                                    preparing_time: None,
                                    picked_up_time: None,
                                    ready_for_pickup_time: None,
                                };
                                insert_into(tickets).values(&ticket).execute(conn)?;

                                Self::reply(
                                    conn,
                                    &kitchen_command.reply_channel,
                                    &kitchen_command.state,
                                    true,
                                    Some({
                                        let reply = CreateTicketCommandReply {
                                            id: ticket.id.to_string(),
                                            sequence: ticket.sequence as i32, // FIXME: i32 to i64
                                        };
                                        let mut buf = Vec::new();
                                        reply.encode(&mut buf).unwrap();
                                        buf
                                    }),
                                )?;

                                Ok(())
                            })
                            .map_err(|_| ())?;

                        Ok(())
                    }
                    ftgo_proto::kitchen_service::kitchen_command::Command::ConfirmCreateTicket(
                        command,
                    ) => {
                        use schema::tickets::dsl::*;
                        let tid = command.id.parse::<Uuid>().expect("Cannot decode ticket_id");
                        let _ = conn
                            .transaction(|conn| {
                                match tickets
                                    .select(models::Ticket::as_select())
                                    .find(tid)
                                    .first::<models::Ticket>(conn)
                                {
                                    Ok(ticket) => {
                                        let succeed = match ticket.state {
                                            models::TicketState::CreatePending => {
                                                update(tickets)
                                                    .set((
                                                        state.eq(
                                                            models::TicketState::AwaitingAcceptance,
                                                        ),
                                                        previous_state.eq(ticket.state),
                                                    ))
                                                    .filter(id.eq(ticket.id))
                                                    .execute(conn)?;

                                                let line_items =
                                                    models::TicketLineItem::belonging_to(&ticket)
                                                        .select(models::TicketLineItem::as_select())
                                                        .load(conn)?;
                                                let mut publisher =
                                                    KitchenEventPublisher::new(conn);
                                                publisher.ticket_created(&ticket, &line_items);

                                                true
                                            }
                                            models::TicketState::AwaitingAcceptance => {
                                                // Already Created
                                                true
                                            }
                                            _ => false,
                                        };

                                        Self::reply(
                                            conn,
                                            &kitchen_command.reply_channel,
                                            &kitchen_command.state,
                                            succeed,
                                            None,
                                        )?;

                                        Ok(())
                                    }
                                    Err(NotFound) => {
                                        Self::reply(
                                            conn,
                                            &kitchen_command.reply_channel,
                                            &kitchen_command.state,
                                            false,
                                            None,
                                        )?;
                                        Ok(())
                                    }
                                    Err(err) => Err(err),
                                }
                            })
                            .map_err(|_| ())?;

                        Ok(())
                    }
                    ftgo_proto::kitchen_service::kitchen_command::Command::CancelCreateTicket(
                        command,
                    ) => {
                        use schema::tickets::dsl::*;
                        let tid = command.id.parse::<Uuid>().expect("Cannot decode ticket_id");
                        let _ = conn
                            .transaction(|conn| {
                                match tickets
                                    .select(models::Ticket::as_select())
                                    .find(tid)
                                    .first::<models::Ticket>(conn)
                                {
                                    Ok(ticket) => {
                                        let succeed = match ticket.state {
                                            models::TicketState::CreatePending => {
                                                update(tickets)
                                                    .set((
                                                        state.eq(models::TicketState::Cancelled),
                                                        previous_state.eq(ticket.state),
                                                    ))
                                                    .filter(id.eq(ticket.id))
                                                    .execute(conn)?;
                                                true
                                            }
                                            models::TicketState::Cancelled => true,
                                            _ => false,
                                        };
                                        Self::reply(
                                            conn,
                                            &kitchen_command.reply_channel,
                                            &kitchen_command.state,
                                            succeed,
                                            None,
                                        )?;
                                        Ok(())
                                    }
                                    Err(NotFound) => {
                                        Self::reply(
                                            conn,
                                            &kitchen_command.reply_channel,
                                            &kitchen_command.state,
                                            false,
                                            None,
                                        )?;
                                        Ok(())
                                    }
                                    Err(err) => Err(err),
                                }
                            })
                            .map_err(|_| ())?;

                        Ok(())
                    }
                }
            }

            AcceptedMessage::RestaurantEvent(restaurant_event) => {
                match restaurant_event.event.unwrap() {
                    restaurant_event::Event::RestaurantCreated(event) => {
                        use schema::restaurant_menu_items::dsl::*;
                        use schema::restaurants::dsl::*;

                        let restaurant = models::Restaurant {
                            id: event.id.parse::<Uuid>().unwrap(),
                        };
                        let menu_items = event
                            .menu_items
                            .into_iter()
                            .map(|i| models::RestaurantMenuItem {
                                restaurant_id: restaurant.id,
                                id: i.id,
                                name: i.name,
                                price: i.price.unwrap().amount.parse().unwrap(),
                            })
                            .collect::<Vec<_>>();

                        conn.transaction(|conn| {
                            insert_into(restaurants)
                                .values(&restaurant)
                                .on_conflict_do_nothing()
                                .execute(conn)?;

                            delete(restaurant_menu_items.filter(
                                schema::restaurant_menu_items::restaurant_id.eq(restaurant.id),
                            ))
                            .execute(conn)?;
                            insert_into(restaurant_menu_items)
                                .values(&menu_items)
                                .execute(conn)?;

                            Ok::<_, diesel::result::Error>(())
                        })
                        .expect("Error while create restaurant");

                        Ok(())
                    }
                    restaurant_event::Event::RestaurantMenuRevised(event) => {
                        use schema::restaurant_menu_items::dsl::*;

                        let rid = event.id.parse::<Uuid>().unwrap();
                        let menu_items = event
                            .menu_items
                            .into_iter()
                            .map(|i| models::RestaurantMenuItem {
                                restaurant_id: rid,
                                id: i.id,
                                name: i.name,
                                price: i.price.unwrap().amount.parse().unwrap(),
                            })
                            .collect::<Vec<_>>();

                        let _ = conn
                            .transaction(|conn| {
                                delete(
                                    restaurant_menu_items.filter(
                                        schema::restaurant_menu_items::restaurant_id.eq(rid),
                                    ),
                                )
                                .execute(conn)?;
                                insert_into(restaurant_menu_items)
                                    .values(&menu_items)
                                    .execute(conn)?;

                                Ok::<_, diesel::result::Error>(())
                            })
                            .expect("Error while revise restaurant menu items");

                        Ok(())
                    }
                }
            }
        }
    }
}

pub fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

    let mut conn = establish_connection();
    let mut consumer = Consumer::from_hosts(vec![kafka_url])
        .with_topic(COMMAND_CHANNEL.to_string())
        .with_topic(RESTAURANT_EVENT_CHANNEL.to_string())
        .with_group(GROUP.to_string())
        .with_fallback_offset(FetchOffset::Earliest)
        .with_offset_storage(Some(GroupOffsetStorage::Kafka))
        .create()
        .unwrap();

    loop {
        let mss = consumer.poll().expect("Cannont poll messages");
        if mss.is_empty() {
            sleep(Duration::from_secs(1));
            continue;
        }

        for ms in mss.iter() {
            for m in ms.messages() {
                match AcceptedMessage::from(ms.topic(), m.value) {
                    Some(message) => {
                        message.process(&mut conn).expect(&format!(
                            "Failed to process message {} {}",
                            ms.topic(),
                            m.offset
                        ));
                    }
                    None => {}
                }
            }
            let _ = consumer.consume_messageset(ms);
        }
        consumer
            .commit_consumed()
            .expect("Error while commit consumed");
    }
}
