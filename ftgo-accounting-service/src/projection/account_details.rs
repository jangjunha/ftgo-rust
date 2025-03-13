use bigdecimal::{BigDecimal, Zero};
use diesel::{expression::AsExpression, insert_into, prelude::*, sql_types::Numeric, update};
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use uuid::Uuid;

use super::{schema::account_details, AccountingProjection, AccountingProjectionError};

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = account_details)]
pub struct AccountDetail {
    pub id: Uuid,
    pub amount: BigDecimal,
    pub version: i64,
    pub last_processed_position: i64,
}

pub struct AccountDetailsProjection<'a> {
    conn: &'a mut PgConnection,
}

impl AccountingProjection for AccountDetailsProjection<'_> {
    fn process(
        &mut self,
        event: &AccountingEvent,
        stream_position: i64,
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
                let entity = AccountDetail {
                    id: aid,
                    amount: BigDecimal::zero(),
                    version: stream_position,
                    last_processed_position: log_position,
                };
                insert_into(account_details::table)
                    .values(&entity)
                    .execute(self.conn)?;
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
                self.conn.transaction(|conn| {
                    if Self::was_already_applied(conn, &aid, log_position)? {
                        return Ok(());
                    };
                    update(account_details::table)
                        .set((
                            account_details::amount.eq(account_details::amount + amount_expr),
                            account_details::version.eq(stream_position),
                            account_details::last_processed_position.eq(log_position),
                        ))
                        .filter(account_details::id.eq(aid))
                        .execute(conn)?;
                    Ok(())
                })
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
                self.conn.transaction(|conn| {
                    if Self::was_already_applied(conn, &aid, log_position)? {
                        return Ok(());
                    };
                    update(account_details::table)
                        .set((
                            account_details::amount.eq(account_details::amount - amount_expr),
                            account_details::version.eq(stream_position),
                            account_details::last_processed_position.eq(log_position),
                        ))
                        .filter(account_details::id.eq(aid))
                        .execute(conn)?;
                    Ok(())
                })
            }
        }
    }
}

impl<'a> AccountDetailsProjection<'a> {
    fn was_already_applied(
        conn: &mut PgConnection,
        account_id: &Uuid,
        log_position: i64,
    ) -> Result<bool, diesel::result::Error> {
        let last = account_details::table
            .select(account_details::last_processed_position)
            .filter(account_details::id.eq(account_id))
            .get_result::<i64>(conn)?;
        Ok(last >= log_position)
    }

    pub fn new(conn: &'a mut PgConnection) -> Self {
        Self { conn }
    }
}
