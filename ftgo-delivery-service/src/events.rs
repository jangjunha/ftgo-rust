use crate::schema;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::delivery_service::{
    delivery_event, DeliveryDropoffEvent, DeliveryEvent, DeliveryPickedUpEvent,
};
use prost::Message;
use prost_types::Timestamp;

pub struct DeliveryEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> DeliveryEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn delivery_picked_up(&mut self, delivery: &models::Delivery) {
        let event = DeliveryEvent {
            event: Some(delivery_event::Event::DeliveryPickedUp(
                DeliveryPickedUpEvent {
                    id: delivery.id.to_string(),
                    picked_up_at: delivery.pickup_time.map(|pickup_time| Timestamp {
                        seconds: pickup_time.timestamp(),
                        nanos: pickup_time.timestamp_subsec_nanos() as i32,
                    }),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: "".to_string(),
                value: buf,
            })
            .execute(self.conn);
    }

    pub fn delivery_dropoff(&mut self, delivery: &models::Delivery) {
        let event = DeliveryEvent {
            event: Some(delivery_event::Event::DeliveryDropoff(
                DeliveryDropoffEvent {
                    id: delivery.id.to_string(),
                    dropoff_at: delivery.delivery_time.map(|delivery_time| Timestamp {
                        seconds: delivery_time.timestamp(),
                        nanos: delivery_time.timestamp_subsec_nanos() as i32,
                    }),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: "".to_string(),
                value: buf,
            })
            .execute(self.conn);
    }
}
