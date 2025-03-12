use std::{
    env,
    thread::sleep,
    time::{Duration, UNIX_EPOCH},
};

use chrono::{DateTime, TimeDelta, Utc};
use diesel::{dsl::insert_into, prelude::*, update, PgConnection};
use dotenvy::dotenv;
use ftgo_delivery_service::{establish_connection, models, schema};
use ftgo_proto::{
    kitchen_service::{kitchen_event, KitchenEvent},
    restaurant_service::{restaurant_event, RestaurantEvent},
};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use rand::seq::IndexedRandom;
use uuid::Uuid;

const RESTAURANT_EVENT_CHANNEL: &'static str = "restaurant.event";
const KITCHEN_EVENT_CHANNEL: &'static str = "kitchen.event";
const GROUP: &'static str = "delivery-service";

enum AcceptedMessage {
    RestaurantEvent(RestaurantEvent),
    KitchenEvent(KitchenEvent),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            RESTAURANT_EVENT_CHANNEL => Some(AcceptedMessage::RestaurantEvent(
                RestaurantEvent::decode(value).expect("Cannot decode restaurant event"),
            )),
            KITCHEN_EVENT_CHANNEL => Some(AcceptedMessage::KitchenEvent(
                KitchenEvent::decode(value).expect("Cannot decode kitchen event"),
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

            AcceptedMessage::KitchenEvent(kitchen_event) => match kitchen_event.event.unwrap {
                kitchen_event::Event::TicketAccepted(event) => {
                    use schema::courier_actions::dsl::*;
                    use schema::couriers::dsl::*;
                    use schema::deliveries::dsl::*;

                    let did = event.id.parse::<Uuid>().expect("Invalid delivery id");
                    let rby = event
                        .ready_by
                        .map(|ts| {
                            let system_time =
                                UNIX_EPOCH + Duration::new(ts.seconds as u64, ts.nanos as u32);
                            DateTime::<Utc>::from(system_time)
                        })
                        .ok_or(())?;

                    let delivery = deliveries
                        .select(models::Delivery::as_select())
                        .find(&did)
                        .first::<models::Delivery>(conn)
                        .map_err(|_| ())?;

                    let mut rng = rand::rng();
                    let candidates = couriers
                        .select(models::Courier::as_select())
                        .filter(available.eq(true))
                        .get_results(conn)
                        .map_err(|_| ())?;

                    if let Some(courier) = candidates.choose(&mut rng) {
                        let actions = vec![
                            models::NewCourierAction {
                                courier_id: courier.id.clone(),
                                type_: models::DeliveryActionType::Pickup,
                                delivery_id: did.clone(),
                                address: delivery.pickup_address.clone(),
                                time: rby.clone(),
                            },
                            models::NewCourierAction {
                                courier_id: courier.id.clone(),
                                type_: models::DeliveryActionType::Dropoff,
                                delivery_id: did.clone(),
                                address: delivery.delivery_address.clone(),
                                time: rby + TimeDelta::minutes(30),
                            },
                        ];
                        insert_into(courier_actions)
                            .values(&actions)
                            .execute(conn)
                            .map_err(|_| ())?;

                        update(deliveries)
                            .set((
                                state.eq(models::DeliveryState::Scheduled),
                                ready_by.eq(rby.clone()),
                                assigned_courier_id.eq(courier.id.clone()),
                            ))
                            .filter(schema::deliveries::id.eq(did))
                            .execute(conn)
                            .map_err(|_| ())?;
                    } else {
                        // No courier exists
                        return Err(());
                    }
                    Ok(())
                }
                kitchen_event::Event::TicketCreated(_) => Ok(()),
                kitchen_event::Event::TicketPreparingStarted(_) => Ok(()),
                kitchen_event::Event::TicketPreparingCompleted(_) => Ok(()),
            },
        }
    }
}

fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

    let mut conn = establish_connection();
    let mut consumer = Consumer::from_hosts(vec![kafka_url])
        .with_topic(RESTAURANT_EVENT_CHANNEL.to_string())
        .with_topic(KITCHEN_EVENT_CHANNEL.to_string())
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
