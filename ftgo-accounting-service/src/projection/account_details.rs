use bigdecimal::{BigDecimal, Zero};
use diesel::{expression::AsExpression, insert_into, prelude::*, sql_types::Numeric, update};
use diesel_async::{
    scoped_futures::ScopedFutureExt, AsyncConnection, AsyncPgConnection, RunQueryDsl,
};
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use uuid::Uuid;

use super::{AccountingProjection, AccountingProjectionError};

use crate::schema::account_details;

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = account_details)]
pub struct AccountDetail {
    pub id: Uuid,
    pub amount: BigDecimal,
    pub version: i64,
    pub last_processed_sequence: i64,
}

pub struct AccountDetailsProjection<'a> {
    conn: &'a mut AsyncPgConnection,
}

impl AccountingProjection for AccountDetailsProjection<'_> {
    async fn process(
        &mut self,
        event: &AccountingEvent,
        sequence: i64,
    ) -> Result<(), AccountingProjectionError> {
        match event.event.as_ref().unwrap() {
            accounting_event::Event::AccountOpened(event) => {
                let aid = event.id.parse::<Uuid>().map_err(|_| {
                    AccountingProjectionError::InvalidEvent {
                        type_: "AccountOpened".to_string(),
                        key: "id".to_string(),
                    }
                })?;
                let entity = AccountDetail {
                    id: aid,
                    amount: BigDecimal::zero(),
                    version: sequence,
                    last_processed_sequence: sequence,
                };
                insert_into(account_details::table)
                    .values(&entity)
                    .execute(self.conn)
                    .await?;
                Ok(())
            }
            accounting_event::Event::AccountDeposited(event) => {
                let aid = event.id.parse::<Uuid>().map_err(|_| {
                    AccountingProjectionError::InvalidEvent {
                        type_: "AccountOpened".to_string(),
                        key: "id".to_string(),
                    }
                })?;
                let amount = event
                    .amount
                    .clone()
                    .ok_or(AccountingProjectionError::InvalidEvent {
                        type_: "AccountDeposited".to_string(),
                        key: "amount".to_string(),
                    })?
                    .amount
                    .parse::<BigDecimal>()
                    .map_err(|_| AccountingProjectionError::InvalidEvent {
                        type_: "AccountDeposited".to_string(),
                        key: "amount".to_string(),
                    })?;
                let amount_expr = AsExpression::<Numeric>::as_expression(amount);
                self.conn
                    .transaction(|conn| {
                        async move {
                            if Self::was_already_applied(conn, &aid, sequence).await? {
                                return Ok(());
                            };
                            update(account_details::table)
                                .set((
                                    account_details::amount
                                        .eq(account_details::amount + amount_expr),
                                    account_details::version.eq(sequence),
                                    account_details::last_processed_sequence.eq(sequence),
                                ))
                                .filter(account_details::id.eq(aid))
                                .execute(conn)
                                .await?;
                            Ok(())
                        }
                        .scope_boxed()
                    })
                    .await
            }
            accounting_event::Event::AccountWithdrawn(event) => {
                let aid = event.id.parse::<Uuid>().map_err(|_| {
                    AccountingProjectionError::InvalidEvent {
                        type_: "AccountOpened".to_string(),
                        key: "id".to_string(),
                    }
                })?;
                let amount = event
                    .amount
                    .clone()
                    .ok_or(AccountingProjectionError::InvalidEvent {
                        type_: "AccountWithdrawn".to_string(),
                        key: "amount".to_string(),
                    })?
                    .amount
                    .parse::<BigDecimal>()
                    .map_err(|_| AccountingProjectionError::InvalidEvent {
                        type_: "AccountWithdrawn".to_string(),
                        key: "amount".to_string(),
                    })?;
                let amount_expr = AsExpression::<Numeric>::as_expression(amount);
                self.conn
                    .transaction(|conn| {
                        async move {
                            if Self::was_already_applied(conn, &aid, sequence).await? {
                                return Ok(());
                            };
                            update(account_details::table)
                                .set((
                                    account_details::amount
                                        .eq(account_details::amount - amount_expr),
                                    account_details::version.eq(sequence),
                                    account_details::last_processed_sequence.eq(sequence),
                                ))
                                .filter(account_details::id.eq(aid))
                                .execute(conn)
                                .await?;
                            Ok(())
                        }
                        .scope_boxed()
                    })
                    .await
            }
            accounting_event::Event::CommandReplyRequested(_) => Ok(()),
        }
    }
}

impl<'a> AccountDetailsProjection<'a> {
    async fn was_already_applied(
        conn: &mut AsyncPgConnection,
        account_id: &Uuid,
        sequence: i64,
    ) -> Result<bool, diesel::result::Error> {
        let last = account_details::table
            .select(account_details::last_processed_sequence)
            .filter(account_details::id.eq(account_id))
            .get_result::<i64>(conn)
            .await?;
        Ok(last >= sequence)
    }

    pub fn new(conn: &'a mut AsyncPgConnection) -> Self {
        Self { conn }
    }
}
