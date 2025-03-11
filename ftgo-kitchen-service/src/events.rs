use crate::schema;
use crate::{models, models::NewOutbox, EVENT_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::kitchen_service::{
    ticket_event, TicketAcceptedEvent, TicketCreatedEvent, TicketDetails, TicketEvent,
    TicketLineItem, TicketPreparingCompletedEvent, TicketPreparingStartedEvent,
};
use prost::Message;
use prost_types::Timestamp;

pub struct KitchenEventPublisher<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> KitchenEventPublisher<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn ticket_created(
        &mut self,
        ticket: &models::Ticket,
        line_items: &Vec<models::TicketLineItem>,
    ) {
        let event = TicketEvent {
            event: Some(ticket_event::Event::TicketCreated(TicketCreatedEvent {
                id: ticket.id.to_string(),
                details: Some(TicketDetails {
                    line_items: line_items
                        .into_iter()
                        .map(|i| TicketLineItem {
                            quantity: i.quantity,
                            menu_item_id: i.menu_item_id.clone(),
                            name: i.name.clone(),
                        })
                        .collect(),
                }),
            })),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: ticket.restaurant_id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }

    pub fn ticket_accepted(&mut self, ticket: &models::Ticket) {
        let event = TicketEvent {
            event: Some(ticket_event::Event::TicketAccepted(TicketAcceptedEvent {
                id: ticket.id.to_string(),
                ready_by: ticket.ready_by.map(|t| Timestamp {
                    seconds: t.timestamp(),
                    nanos: t.timestamp_subsec_nanos() as i32,
                }),
            })),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: ticket.restaurant_id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }

    pub fn ticket_preparing_started(&mut self, ticket: &models::Ticket) {
        let event = TicketEvent {
            event: Some(ticket_event::Event::TicketPreparingStarted(
                TicketPreparingStartedEvent {
                    id: ticket.id.to_string(),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: ticket.restaurant_id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }

    pub fn ticket_preparing_completed(&mut self, ticket: &models::Ticket) {
        let event = TicketEvent {
            event: Some(ticket_event::Event::TicketPreparingCompleted(
                TicketPreparingCompletedEvent {
                    id: ticket.id.to_string(),
                },
            )),
        };
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        let _ = diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: EVENT_CHANNEL.to_string(),
                key: ticket.restaurant_id.to_string(),
                value: buf,
            })
            .execute(self.conn);
    }
}
