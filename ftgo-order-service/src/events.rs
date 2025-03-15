use crate::schema;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::order_service::{
    order_event, OrderAuthorizedEvent, OrderEvent, OrderRejectedEvent,
};
use prost::Message;

pub struct OrderEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> OrderEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn order_authorized(&mut self, order: &models::Order) {
        let event = OrderEvent {
            event: Some(order_event::Event::OrderAuthorized(OrderAuthorizedEvent {
                id: order.id.to_string(),
            })),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: order.id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }

    pub fn order_rejected(&mut self, order: &models::Order) {
        let event = OrderEvent {
            event: Some(order_event::Event::OrderRejected(OrderRejectedEvent {
                id: order.id.to_string(),
            })),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: order.id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }
}
