use std::fmt::Display;

use bigdecimal::{BigDecimal, Zero};
use ftgo_proto::{
    accounting_service::{
        accounting_event, AccountDeposited, AccountOpened, AccountWithdrawn, AccountingEvent,
    },
    common::Money,
};
use uuid::Uuid;

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
