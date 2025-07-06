use crate::models::NewOutbox;
use crate::{schema, REPLY_CHANNEL};
use diesel::{prelude::*, PgConnection};
use ftgo_proto::order_service::{
    order_command, ApproveOrderCommand, OrderCommand, RejectOrderCommand,
};
use prost::Message;
use std::collections::HashMap;
use uuid::Uuid;

const ORDER_COMMAND_CHANNEL: &str = "order.command";

pub struct OrderServiceProxy<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> OrderServiceProxy<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn approve_order(
        &mut self,
        order_id: &Uuid,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = order_command::Command::Approve(ApproveOrderCommand {
            id: order_id.to_string(),
        });
        self.publish(command, state, order_id)
    }

    pub fn reject_order(
        &mut self,
        order_id: &Uuid,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = order_command::Command::Reject(RejectOrderCommand {
            id: order_id.to_string(),
        });
        self.publish(command, state, order_id)
    }

    fn publish(
        &mut self,
        command: order_command::Command,
        state: &HashMap<String, String>,
        order_id: &Uuid,
    ) -> Result<(), diesel::result::Error> {
        let command = OrderCommand {
            state: state.clone(),
            reply_channel: Some(REPLY_CHANNEL.to_string()),
            command: Some(command),
        };

        let mut buf = Vec::new();
        command.encode(&mut buf).unwrap();

        diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: ORDER_COMMAND_CHANNEL.to_string(),
                key: order_id.to_string(),
                value: buf,
            })
            .execute(self.conn)
            .map(|_| ())
    }
}
