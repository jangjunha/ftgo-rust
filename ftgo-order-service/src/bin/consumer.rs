use std::{collections::HashMap, env, thread::sleep, time::Duration};

use diesel::{delete, insert_into, prelude::*, Connection, ExpressionMethods, PgConnection};
use dotenvy::dotenv;
use ftgo_order_service::{
    command_handlers::handle_command,
    establish_connection,
    models::{self, NewOutbox},
    schema, COMMAND_CHANNEL,
};
use ftgo_proto::{
    common::CommandReply,
    order_service::OrderCommand,
    restaurant_service::{restaurant_event, RestaurantEvent},
};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const GROUP: &'static str = "order-service";

const RESTAURANT_EVENT_CHANNEL: &'static str = "restaurant.event";

enum AcceptedMessage {
    OrderCommand(OrderCommand),
    RestaurantEvent(RestaurantEvent),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            COMMAND_CHANNEL => Some(AcceptedMessage::OrderCommand(
                OrderCommand::decode(value).expect("Cannot decode order command"),
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
            AcceptedMessage::OrderCommand(order_command) => conn
                .transaction(|conn| match handle_command(order_command.clone(), conn) {
                    Ok(()) => Self::reply(
                        conn,
                        &order_command.reply_channel,
                        &order_command.state,
                        true,
                        None,
                    ),
                    Err(_) => Self::reply(
                        conn,
                        &order_command.reply_channel,
                        &order_command.state,
                        false,
                        None,
                    ),
                })
                .map_err(|_| ()),

            AcceptedMessage::RestaurantEvent(restaurant_event) => {
                match restaurant_event.event.unwrap() {
                    restaurant_event::Event::RestaurantCreated(event) => {
                        let restaurant = models::Restaurant {
                            id: event.id.parse::<Uuid>().unwrap(),
                            name: event.name.to_string(),
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
                            insert_into(schema::restaurants::table)
                                .values(&restaurant)
                                .on_conflict_do_nothing()
                                .execute(conn)?;

                            delete(schema::restaurant_menu_items::table.filter(
                                schema::restaurant_menu_items::restaurant_id.eq(restaurant.id),
                            ))
                            .execute(conn)?;
                            insert_into(schema::restaurant_menu_items::table)
                                .values(&menu_items)
                                .execute(conn)?;

                            Ok::<_, diesel::result::Error>(())
                        })
                        .expect("Error while create restaurant");

                        Ok(())
                    }
                    restaurant_event::Event::RestaurantMenuRevised(event) => {
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
                                    schema::restaurant_menu_items::table.filter(
                                        schema::restaurant_menu_items::restaurant_id.eq(rid),
                                    ),
                                )
                                .execute(conn)?;
                                insert_into(schema::restaurant_menu_items::table)
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

fn main() {
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
