use bigdecimal::{BigDecimal, Zero};
use diesel::{expression::AsExpression, insert_into, prelude::*, sql_types::Numeric, update};
use diesel_async::{
    scoped_futures::ScopedFutureExt, AsyncConnection, AsyncPgConnection, RunQueryDsl,
};
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use uuid::Uuid;

use super::{schema::account_infos, AccountingProjection, AccountingProjectionError};

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = account_infos)]
pub struct AccountInfo {
    pub id: Uuid,
    pub deposit_accumulate: BigDecimal,
    pub deposit_count: i32,
    pub withdraw_accumulate: BigDecimal,
    pub withdraw_count: i32,
    pub last_processed_position: i64,
}

pub struct AccountInfosProjection<'a> {
    conn: &'a mut AsyncPgConnection,
}

impl AccountingProjection for AccountInfosProjection<'_> {
    async fn process(
        &mut self,
        event: &AccountingEvent,
        _: i64,
        log_position: i64,
    ) -> Result<(), AccountingProjectionError> {
        match event.event.as_ref().unwrap() {
            accounting_event::Event::AccountOpened(event) => {
                let aid = event.id.parse::<Uuid>().map_err(|_| {
                    AccountingProjectionError::InvalidEvent {
                        type_: "AccountOpened".to_string(),
                        key: "id".to_string(),
                    }
                })?;
                let entity = AccountInfo {
                    id: aid,
                    deposit_accumulate: BigDecimal::zero(),
                    deposit_count: 0,
                    withdraw_accumulate: BigDecimal::zero(),
                    withdraw_count: 0,
                    last_processed_position: log_position,
                };
                insert_into(account_infos::table)
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
                            if Self::was_already_applied(conn, &aid, log_position).await? {
                                return Ok(());
                            };
                            update(account_infos::table)
                                .set((
                                    account_infos::deposit_accumulate
                                        .eq(account_infos::deposit_accumulate + amount_expr),
                                    account_infos::deposit_count
                                        .eq(account_infos::deposit_count + 1),
                                    account_infos::last_processed_position.eq(log_position),
                                ))
                                .filter(account_infos::id.eq(aid))
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
                            if Self::was_already_applied(conn, &aid, log_position).await? {
                                return Ok(());
                            };
                            update(account_infos::table)
                                .set((
                                    account_infos::withdraw_accumulate
                                        .eq(account_infos::withdraw_accumulate + amount_expr),
                                    account_infos::withdraw_count
                                        .eq(account_infos::withdraw_count + 1),
                                    account_infos::last_processed_position.eq(log_position),
                                ))
                                .filter(account_infos::id.eq(aid))
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

impl<'a> AccountInfosProjection<'a> {
    async fn was_already_applied(
        conn: &mut AsyncPgConnection,
        account_id: &Uuid,
        log_position: i64,
    ) -> Result<bool, diesel::result::Error> {
        let last = account_infos::table
            .select(account_infos::last_processed_position)
            .filter(account_infos::id.eq(account_id))
            .get_result::<i64>(conn)
            .await?;
        Ok(last >= log_position)
    }

    pub fn new(conn: &'a mut AsyncPgConnection) -> Self {
        Self { conn }
    }
}
