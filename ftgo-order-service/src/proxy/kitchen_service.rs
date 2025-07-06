use std::collections::HashMap;

use crate::models::NewOutbox;
use crate::{schema, REPLY_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::kitchen_service::{
    kitchen_command, CancelCreateTicketCommand, ConfirmCreateTicketCommand, CreateTicketCommand,
    KitchenCommand, TicketDetails,
};
use prost::Message;
use uuid::Uuid;

const KITCHEN_COMMAND_CHANNEL: &str = "kitchen.command";

pub struct KitchenServiceProxy<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> KitchenServiceProxy<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn create_ticket(
        &mut self,
        id: &Uuid,
        details: &TicketDetails,
        restaurant_id: &Uuid,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = kitchen_command::Command::CreateTicket(CreateTicketCommand {
            id: id.to_string(),
            details: Some(details.clone()),
            restaurant_id: restaurant_id.to_string(),
        });
        self.publish(command, state, restaurant_id)
    }

    pub fn cancel_create_ticket(
        &mut self,
        id: &Uuid,
        restaurant_id: &Uuid,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = kitchen_command::Command::CancelCreateTicket(CancelCreateTicketCommand {
            id: id.to_string(),
        });
        self.publish(command, state, restaurant_id)
    }

    pub fn confirm_create_ticket(
        &mut self,
        id: &Uuid,
        restaurant_id: &Uuid,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = kitchen_command::Command::ConfirmCreateTicket(ConfirmCreateTicketCommand {
            id: id.to_string(),
        });
        self.publish(command, state, restaurant_id)
    }

    fn publish(
        &mut self,
        command: kitchen_command::Command,
        state: &HashMap<String, String>,
        restaurant_id: &Uuid,
    ) -> Result<(), diesel::result::Error> {
        let command = KitchenCommand {
            state: state.clone(),
            reply_channel: Some(REPLY_CHANNEL.to_string()),
            command: Some(command),
        };

        let mut buf = Vec::new();
        command.encode(&mut buf).unwrap();

        diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: KITCHEN_COMMAND_CHANNEL.to_string(),
                key: restaurant_id.to_string(),
                value: buf,
            })
            .execute(self.conn)
            .map(|_| ())
    }
}
