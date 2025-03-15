use diesel::{prelude::*, update, Connection, PgConnection};
use ftgo_proto::order_service::{order_command::Command, OrderCommand};
use thiserror::Error;
use uuid::Uuid;

use crate::{events::OrderEventPublisher, models, schema};

pub fn handle_command(
    order_command: OrderCommand,
    conn: &mut PgConnection,
) -> Result<(), CommandHandlerError> {
    conn.transaction(|conn| match order_command.command.unwrap() {
        Command::Approve(command) => {
            let oid = command.id.parse::<Uuid>().expect("Invalid order id");
            let order = schema::orders::table
                .select(models::Order::as_select())
                .find(&oid)
                .for_update()
                .get_result::<models::Order>(conn)?;
            if order.state != models::OrderState::ApprovalPending {
                return Err(CommandHandlerError::InvalidState {
                    current: order.state,
                    expect: models::OrderState::ApprovalPending,
                });
            }

            update(schema::orders::table)
                .set(schema::orders::state.eq(models::OrderState::Approved))
                .filter(schema::orders::id.eq(&oid))
                .execute(conn)?;

            let mut publisher = OrderEventPublisher::new(conn);
            publisher.order_authorized(&order)?;

            Ok(())
        }

        Command::Reject(command) => {
            let oid = command.id.parse::<Uuid>().expect("Invalid order id");
            let order = schema::orders::table
                .select(models::Order::as_select())
                .find(&oid)
                .for_update()
                .get_result::<models::Order>(conn)?;
            if order.state != models::OrderState::ApprovalPending {
                return Err(CommandHandlerError::InvalidState {
                    current: order.state,
                    expect: models::OrderState::ApprovalPending,
                });
            }

            update(schema::orders::table)
                .set(schema::orders::state.eq(models::OrderState::Rejected))
                .filter(schema::orders::id.eq(&oid))
                .execute(conn)?;

            let mut publisher = OrderEventPublisher::new(conn);
            publisher.order_authorized(&order)?;

            Ok(())
        }
    })
}

#[derive(Error, Debug)]
pub enum CommandHandlerError {
    #[error("Invalid current state {current:?}, expected {expect:?}")]
    InvalidState {
        current: models::OrderState,
        expect: models::OrderState,
    },
    #[error("Unexpected internal error")]
    Internal(#[from] diesel::result::Error),
}
