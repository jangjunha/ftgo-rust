use std::collections::HashMap;

use bigdecimal::BigDecimal;
use diesel::{prelude::*, ExpressionMethods, SelectableHelper};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use eventstore::ExpectedRevision;
use ftgo_proto::{
    accounting_service::{accounting_event, AccountingEvent, CommandReplyRequested},
    common::CommandReply,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{models, projection, store::AccountStore};

pub struct AccountingService<'a> {
    store: AccountStore,
    conn: &'a mut AsyncPgConnection,
}

impl AccountingService<'_> {
    pub async fn create(
        &self,
        account_id: Option<Uuid>,
    ) -> Result<models::Account, AccountingError> {
        let account_id = account_id.unwrap_or_else(|| Uuid::new_v4());
        let account = models::Account::new(account_id);

        let event = account.open().map_err(|_| AccountingError::Internal)?;

        let _ = self
            .store
            .append(
                &account_id,
                &vec![(None, event.clone())],
                ExpectedRevision::NoStream,
            )
            .await;

        Ok(account.apply(event))
    }

    pub async fn deposit(
        &self,
        account_id: Uuid,
        amount: BigDecimal,
        description: Option<String>,
        event_id: Option<Uuid>,
        command_metadata: Option<(&str, &HashMap<String, String>)>,
    ) -> Result<models::Account, AccountingError> {
        let account = self
            .store
            .get(&account_id)
            .await
            .map_err(|_| AccountingError::Internal)?;

        let events = match account.deposit(amount, description) {
            Ok(event) => {
                let mut events = vec![(event_id, event)];
                if let Some((reply_channel, state)) = command_metadata {
                    events.push((
                        None,
                        AccountingEvent {
                            event: Some(accounting_event::Event::CommandReplyRequested(
                                CommandReplyRequested {
                                    reply: Some(CommandReply {
                                        state: state.clone(),
                                        succeed: true,
                                        body: None,
                                    }),
                                    reply_channel: reply_channel.to_string(),
                                },
                            )),
                        },
                    ));
                }
                events
            }
            Err(_) => {
                if let Some((reply_channel, state)) = command_metadata {
                    vec![(
                        None,
                        AccountingEvent {
                            event: Some(accounting_event::Event::CommandReplyRequested(
                                CommandReplyRequested {
                                    reply: Some(CommandReply {
                                        state: state.clone(),
                                        succeed: false,
                                        body: None,
                                    }),
                                    reply_channel: reply_channel.to_string(),
                                },
                            )),
                        },
                    )]
                } else {
                    vec![]
                }
            }
        };

        let _ = self
            .store
            .append(&account_id, &events, ExpectedRevision::StreamExists)
            .await;

        Ok(events.into_iter().fold(account, |acc, (_, e)| acc.apply(e)))
    }

    pub async fn withdraw(
        &self,
        account_id: Uuid,
        amount: BigDecimal,
        description: Option<String>,
        event_id: Option<Uuid>,
        command_metadata: Option<(&str, &HashMap<String, String>)>,
    ) -> Result<models::Account, AccountingError> {
        let account = self
            .store
            .get(&account_id)
            .await
            .map_err(|_| AccountingError::Internal)?;

        let events = match account.withdraw(amount, description) {
            Ok(event) => {
                let mut events = vec![(event_id, event)];
                if let Some((reply_channel, state)) = command_metadata {
                    events.push((
                        None,
                        AccountingEvent {
                            event: Some(accounting_event::Event::CommandReplyRequested(
                                CommandReplyRequested {
                                    reply: Some(CommandReply {
                                        state: state.clone(),
                                        succeed: true,
                                        body: None,
                                    }),
                                    reply_channel: reply_channel.to_string(),
                                },
                            )),
                        },
                    ));
                }
                events
            }
            Err(_) => {
                if let Some((reply_channel, state)) = command_metadata {
                    vec![(
                        None,
                        AccountingEvent {
                            event: Some(accounting_event::Event::CommandReplyRequested(
                                CommandReplyRequested {
                                    reply: Some(CommandReply {
                                        state: state.clone(),
                                        succeed: false,
                                        body: None,
                                    }),
                                    reply_channel: reply_channel.to_string(),
                                },
                            )),
                        },
                    )]
                } else {
                    vec![]
                }
            }
        };

        let _ = self
            .store
            .append(&account_id, &events, ExpectedRevision::StreamExists)
            .await;

        Ok(events.into_iter().fold(account, |acc, (_, e)| acc.apply(e)))
    }

    pub async fn get_account(
        &mut self,
        account_id: &Uuid,
    ) -> Result<Option<projection::account_details::AccountDetail>, AccountingError> {
        use projection::account_details::AccountDetail;
        use projection::schema::account_details;
        match account_details::table
            .select(AccountDetail::as_select())
            .find(account_id)
            .get_result(self.conn)
            .await
        {
            Ok(entity) => Ok(Some(entity)),
            Err(diesel::result::Error::NotFound) => Ok(None),
            Err(_) => Err(AccountingError::Internal),
        }
    }

    pub async fn list_accounts(
        &mut self,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<projection::account_infos::AccountInfo>, AccountingError> {
        use projection::account_infos::AccountInfo;
        use projection::schema::account_infos;
        account_infos::table
            .select(AccountInfo::as_select())
            .order(account_infos::id.asc())
            .offset(((page - 1) * page_size).into())
            .limit(page_size.into())
            .get_results(self.conn)
            .await
            .map_err(|_| AccountingError::Internal)
    }
}

impl<'a> AccountingService<'a> {
    pub fn new(store: AccountStore, conn: &'a mut AsyncPgConnection) -> Self {
        Self { store, conn }
    }
}

#[derive(Error, Debug)]
pub enum AccountingError {
    #[error("Internal error")]
    Internal,
}
