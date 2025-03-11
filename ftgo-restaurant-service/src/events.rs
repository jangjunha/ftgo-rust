use crate::schema;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::common::Money;
use ftgo_proto::restaurant_service::{
    restaurant_event, MenuItem, RestaurantCreatedEvent, RestaurantEvent,
};
use prost::Message;

pub struct RestaurantEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> RestaurantEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn restaurant_created(
        &mut self,
        restaurant: &models::Restaurant,
        menu_items: &Vec<models::RestaurantMenuItem>,
    ) {
        let event = RestaurantEvent {
            event: Some(restaurant_event::Event::RestaurantCreated(
                RestaurantCreatedEvent {
                    id: restaurant.id.to_string(),
                    name: restaurant.name.to_string(),
                    address: restaurant.address.to_string(),
                    menu_items: menu_items
                        .into_iter()
                        .map(|i| MenuItem {
                            id: i.id.clone(),
                            name: i.name.clone(),
                            price: Some(Money {
                                amount: i.price.to_string(),
                            }),
                        })
                        .collect(),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: restaurant.id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }
}
