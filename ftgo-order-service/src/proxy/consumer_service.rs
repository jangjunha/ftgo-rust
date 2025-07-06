use crate::models::NewOutbox;
use crate::{schema, REPLY_CHANNEL};
use bigdecimal::BigDecimal;
use diesel::{prelude::*, PgConnection};
use ftgo_proto::consumer_service::{
    consumer_command, ConsumerCommand, ValidateOrderByConsumerCommand,
};
use prost::Message;
use std::collections::HashMap;
use uuid::Uuid;

const CONSUMER_COMMAND_CHANNEL: &str = "consumer.command";

pub struct ConsumerServiceProxy<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> ConsumerServiceProxy<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn validate_order_by_consumer(
        &mut self,
        consumer_id: &Uuid,
        order_id: &Uuid,
        order_total: &BigDecimal,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command =
            consumer_command::Command::ValidateOrderByConsumer(ValidateOrderByConsumerCommand {
                id: consumer_id.to_string(),
                order_id: order_id.to_string(),
                order_total: Some(ftgo_proto::common::Money {
                    amount: order_total.to_string(),
                }),
            });
        self.publish(command, state, consumer_id)
    }

    fn publish(
        &mut self,
        command: consumer_command::Command,
        state: &HashMap<String, String>,
        consumer_id: &Uuid,
    ) -> Result<(), diesel::result::Error> {
        let command = ConsumerCommand {
            state: state.clone(),
            reply_channel: Some(REPLY_CHANNEL.to_string()),
            command: Some(command),
        };

        let mut buf = Vec::new();
        command.encode(&mut buf).unwrap();

        diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: CONSUMER_COMMAND_CHANNEL.to_string(),
                key: consumer_id.to_string(),
                value: buf,
            })
            .execute(self.conn)
            .map(|_| ())
    }
}
