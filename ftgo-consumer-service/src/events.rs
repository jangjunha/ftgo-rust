use crate::schema;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::consumer_service::{consumer_event, ConsumerCreatedEvent, ConsumerEvent};
use prost::Message;

pub struct ConsumerEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> ConsumerEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn consumer_created(&mut self, consumer: &models::Consumer) {
        let event = ConsumerEvent {
            event: Some(consumer_event::Event::ConsumerCreated(
                ConsumerCreatedEvent {
                    id: consumer.id.to_string(),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: consumer.id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }
}
