use crate::schema;
use crate::serializer::serialize_order_details;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::order_service::{
    order_event, OrderAuthorizedEvent, OrderCreatedEvent, OrderEvent, OrderRejectedEvent,
};
use prost::Message;
use uuid::Uuid;

pub struct OrderEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> OrderEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn order_created(
        &mut self,
        order: &models::Order,
        line_items: &Vec<models::OrderLineItem>,
        restaurant: &models::Restaurant,
    ) -> Result<(), diesel::result::Error> {
        let event = OrderEvent {
            event: Some(order_event::Event::OrderCreated(OrderCreatedEvent {
                id: order.id.to_string(),
                order_details: Some(serialize_order_details(order, line_items, restaurant)),
                delivery_address: order.delivery_address.to_string(),
                restaurant_name: restaurant.name.to_string(),
            })),
        };
        self.publish(event, &order.id)
    }

    pub fn order_authorized(&mut self, order: &models::Order) -> Result<(), diesel::result::Error> {
        let event = OrderEvent {
            event: Some(order_event::Event::OrderAuthorized(OrderAuthorizedEvent {
                id: order.id.to_string(),
            })),
        };
        self.publish(event, &order.id)
    }

    pub fn order_rejected(&mut self, order: &models::Order) -> Result<(), diesel::result::Error> {
        let event = OrderEvent {
            event: Some(order_event::Event::OrderRejected(OrderRejectedEvent {
                id: order.id.to_string(),
            })),
        };
        self.publish(event, &order.id)
    }

    fn publish(&mut self, event: OrderEvent, order_id: &Uuid) -> Result<(), diesel::result::Error> {
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: order_id.to_string(),
                value: buf,
            })
            .execute(self.conn)
            .map(|_| ())
    }
}
