use crate::models::NewOutbox;
use crate::{schema, REPLY_CHANNEL};
use bigdecimal::BigDecimal;
use diesel::{prelude::*, PgConnection};
use ftgo_proto::accounting_service::{
    accounting_command, AccountingCommand, DepositCommand, WithdrawCommand,
};
use prost::Message;
use std::collections::HashMap;
use uuid::Uuid;

const ACCOUNTING_COMMAND_CHANNEL: &str = "accounting.command";

pub struct AccountingServiceProxy<'a> {
    conn: &'a mut PgConnection,
}

impl<'a> AccountingServiceProxy<'a> {
    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }

    pub fn withdraw(
        &mut self,
        consumer_id: &Uuid,
        order_id: &Uuid,
        amount: &BigDecimal,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = accounting_command::Command::Withdraw(WithdrawCommand {
            id: consumer_id.to_string(),
            amount: Some(ftgo_proto::common::Money {
                amount: amount.to_string(),
            }),
            description: Some(format!("Order {}", order_id)),
        });
        self.publish(command, state, consumer_id)
    }

    pub fn deposit(
        &mut self,
        consumer_id: &Uuid,
        order_id: &Uuid,
        amount: &BigDecimal,
        state: &HashMap<String, String>,
    ) -> Result<(), diesel::result::Error> {
        let command = accounting_command::Command::Deposit(DepositCommand {
            id: consumer_id.to_string(),
            amount: Some(ftgo_proto::common::Money {
                amount: amount.to_string(),
            }),
            description: Some(format!("Order {}", order_id)),
        });
        self.publish(command, state, consumer_id)
    }

    fn publish(
        &mut self,
        command: accounting_command::Command,
        state: &HashMap<String, String>,
        consumer_id: &Uuid,
    ) -> Result<(), diesel::result::Error> {
        let command = AccountingCommand {
            state: state.clone(),
            reply_channel: Some(REPLY_CHANNEL.to_string()),
            command: Some(command),
        };

        let mut buf = Vec::new();
        command.encode(&mut buf).unwrap();

        diesel::insert_into(schema::outbox::table)
            .values(NewOutbox {
                topic: ACCOUNTING_COMMAND_CHANNEL.to_string(),
                key: consumer_id.to_string(),
                value: buf,
            })
            .execute(self.conn)
            .map(|_| ())
    }
}
