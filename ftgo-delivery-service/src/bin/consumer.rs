use std::{env, thread::sleep, time::Duration};

use diesel::{dsl::insert_into, prelude::*, PgConnection};
use dotenvy::dotenv;
use ftgo_delivery_service::{establish_connection, models, schema};
use ftgo_proto::restaurant_service::{restaurant_event, RestaurantEvent};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const RESTAURANT_EVENT_CHANNEL: &'static str = "restaurant.event";
const GROUP: &'static str = "delivery-service";

enum AcceptedMessage {
    RestaurantEvent(RestaurantEvent),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            RESTAURANT_EVENT_CHANNEL => Some(AcceptedMessage::RestaurantEvent(
                RestaurantEvent::decode(value).expect("Cannot decode restaurant event"),
            )),
            _ => None,
        }
    }

    fn process(self, conn: &mut PgConnection) -> Result<(), ()> {
        match self {
            AcceptedMessage::RestaurantEvent(restaurant_event) => {
                match restaurant_event.event.unwrap() {
                    restaurant_event::Event::RestaurantCreated(event) => {
                        use schema::restaurants::dsl::*;

                        let restaurant = models::Restaurant {
                            id: event.id.parse::<Uuid>().unwrap(),
                            name: event.name.to_string(),
                            address: event.address.to_string(),
                        };

                        insert_into(restaurants)
                            .values(&restaurant)
                            .on_conflict_do_nothing()
                            .execute(conn)
                            .expect("Error wile insert restaurant");

                        Ok(())
                    }
                    restaurant_event::Event::RestaurantMenuRevised(_) => Ok(()),
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
