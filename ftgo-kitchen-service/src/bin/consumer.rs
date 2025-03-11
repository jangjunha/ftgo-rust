use std::{env, thread::sleep, time::Duration};

use diesel::{
    delete, dsl::insert_into, query_dsl::methods::FilterDsl, Connection, ExpressionMethods,
    PgConnection, RunQueryDsl,
};
use dotenvy::dotenv;
use ftgo_kitchen_service::{establish_connection, models, schema};
use ftgo_proto::restaurant_service::{restaurant_event, RestaurantEvent};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const GROUP: &'static str = "kitchen-service";

const RESTAURANT_EVENT_TOPIC: &'static str = "restaurant.event";

enum Topic {
    RestaurantEvent(RestaurantEvent),
}

impl Topic {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            RESTAURANT_EVENT_TOPIC => Some(Topic::RestaurantEvent(
                RestaurantEvent::decode(value).expect("Cannot decode restaurant event"),
            )),
            _ => None,
        }
    }

    fn process(self, conn: &mut PgConnection) {
        match self {
            Topic::RestaurantEvent(restaurant_event) => match restaurant_event.event.unwrap() {
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
                                restaurant_menu_items
                                    .filter(schema::restaurant_menu_items::restaurant_id.eq(rid)),
                            )
                            .execute(conn)?;
                            insert_into(restaurant_menu_items)
                                .values(&menu_items)
                                .execute(conn)?;

                            Ok::<_, diesel::result::Error>(())
                        })
                        .expect("Error while revise restaurant menu items");
                }
            },
        }
    }
}

fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

    let mut conn = establish_connection();
    let mut consumer = Consumer::from_hosts(vec![kafka_url])
        .with_topic(RESTAURANT_EVENT_TOPIC.to_string())
        .with_group(GROUP.to_string())
        .with_fallback_offset(FetchOffset::Earliest)
        .with_offset_storage(Some(GroupOffsetStorage::Kafka))
        .create()
        .unwrap();

    loop {
        let mss = consumer.poll().expect("Cannont poll messages");
        if mss.is_empty() {
            println!("empty");
            sleep(Duration::from_secs(1));
            continue;
        }

        for ms in mss.iter() {
            for m in ms.messages() {
                println!("m");
                match Topic::from(ms.topic(), m.value) {
                    Some(topic) => {
                        println!("p");
                        topic.process(&mut conn);
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
