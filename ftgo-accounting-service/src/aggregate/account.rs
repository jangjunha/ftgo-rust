use std::fmt::Display;

use bigdecimal::{BigDecimal, Zero};
use diesel_async::AsyncPgConnection;
use ftgo_proto::{
    accounting_service::{
        accounting_event, AccountDeposited, AccountOpened, AccountWithdrawn, AccountingEvent,
    },
    common::Money,
};
use futures::TryStreamExt;
use kafka::producer::AsBytes;
use prost::Message;
use serde_json::json;
use uuid::Uuid;

use crate::store::event::{AppendCondition, EventData, EventStore, EventStoreError};

#[derive(Clone)]
pub struct Account {
    pub id: Uuid,
    pub balance: BigDecimal,
}

impl Account {
    pub fn open(&self) -> Result<AccountingEvent, AccountError> {
        Ok(AccountingEvent {
            event: Some(accounting_event::Event::AccountOpened(AccountOpened {
                id: self.id.to_string(),
            })),
        })
    }

    pub fn deposit(
        &self,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Result<AccountingEvent, AccountError> {
        Ok(AccountingEvent {
            event: Some(accounting_event::Event::AccountDeposited(
                AccountDeposited {
                    id: self.id.to_string(),
                    amount: Some(Money {
                        amount: amount.to_string(),
                    }),
                    description: description,
                },
            )),
        })
    }

    pub fn withdraw(
        &self,
        amount: BigDecimal,
        description: Option<String>,
    ) -> Result<AccountingEvent, AccountError> {
        if amount > self.balance {
            return Err(AccountError::AccountLimitExceeded {
                requested: amount,
                balance: self.balance.clone(),
            });
        }
        Ok(AccountingEvent {
            event: Some(accounting_event::Event::AccountWithdrawn(
                AccountWithdrawn {
                    id: self.id.to_string(),
                    amount: Some(Money {
                        amount: amount.to_string(),
                    }),
                    description: description,
                },
            )),
        })
    }
}

impl Account {
    pub fn new(id: Uuid) -> Self {
        Self {
            id: id,
            balance: BigDecimal::zero(),
        }
    }

    pub fn apply(&self, accounting_event: AccountingEvent) -> Self {
        match accounting_event.event.unwrap() {
            accounting_event::Event::AccountOpened(event) => Self {
                id: event.id.parse().expect("Invalid account id"),
                balance: BigDecimal::zero(),
            },
            accounting_event::Event::AccountDeposited(event) => Self {
                id: self.id,
                balance: self.balance.clone()
                    + event
                        .amount
                        .unwrap()
                        .amount
                        .parse::<BigDecimal>()
                        .expect("Invalid amount"),
            },
            accounting_event::Event::AccountWithdrawn(event) => Self {
                id: self.id,
                balance: self.balance.clone()
                    - event
                        .amount
                        .unwrap()
                        .amount
                        .parse::<BigDecimal>()
                        .expect("Invalid amount"),
            },
            accounting_event::Event::CommandReplyRequested(_) => self.to_owned(),
        }
    }
}

pub enum AccountError {
    AccountLimitExceeded {
        requested: BigDecimal,
        balance: BigDecimal,
    },
}

impl Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::AccountLimitExceeded { requested, balance } => f.write_fmt(format_args!(
                "Requested amount {} is greater than balance {}",
                requested, balance
            )),
        }
    }
}

pub struct AccountStore<'a> {
    conn: &'a mut AsyncPgConnection,
}

impl<'a> AccountStore<'a> {
    pub async fn append(
        &mut self,
        id: &Uuid,
        events: &Vec<(Option<Uuid>, AccountingEvent)>,
        condition: Option<AppendCondition>,
    ) -> Result<(), EventStoreError> {
        let stream_id = format!("Account-{}", id);
        let mut client = EventStore::new(&stream_id, self.conn);
        client
            .append(
                events
                    .iter()
                    .map(|(event_id, accounting_event)| {
                        let event_type = match accounting_event.event.as_ref().unwrap() {
                            accounting_event::Event::AccountOpened(_) => "AccountOpened",
                            accounting_event::Event::AccountDeposited(_) => "AccountDeposited",
                            accounting_event::Event::AccountWithdrawn(_) => "AccountWithdrawn",
                            accounting_event::Event::CommandReplyRequested(_) => {
                                "CommandReplyRequested"
                            }
                        };
                        let event = EventData {
                            payload: {
                                let mut buf = Vec::new();
                                accounting_event.encode(&mut buf).unwrap();
                                buf
                            }
                            .into(),
                            metadata: json!({"event_type": event_type}),
                        };
                        (event_id.as_ref(), event)
                    })
                    .collect::<Vec<_>>(),
                condition,
            )
            .await?;
        Ok(())
    }

    pub async fn get(&mut self, id: &Uuid) -> Result<(Account, i64), EventStoreError> {
        let stream_id = format!("Account-{}", id);
        let mut client = EventStore::new(&stream_id, self.conn);
        let stream = client.read_stream().await?;
        let account = stream
            .try_fold(
                (Account::new(id.clone()), -1),
                async |(account, _), event| {
                    let sequence = event.sequence;
                    let event = AccountingEvent::decode(event.payload.as_bytes())
                        .expect("Invalid accounting event");
                    Ok((account.apply(event), sequence))
                },
            )
            .await?;
        Ok(account)
    }

    pub fn new(conn: &'a mut AsyncPgConnection) -> Self {
        Self { conn }
    }
}
