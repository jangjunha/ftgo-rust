use std::io::Write;

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use diesel::{
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    pg::{Pg, PgValue},
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
};
use uuid::Uuid;

use crate::schema::{outbox, restaurant_menu_items, restaurants, ticket_line_items, tickets};

#[derive(FromSqlRow, AsExpression, PartialEq, Copy, Clone, Debug)]
#[diesel(sql_type = crate::schema::sql_types::TicketState)]
pub enum TicketState {
    CreatePending,
    AwaitingAcceptance,
    Accepted,
    Preparing,
    ReadyForPickup,
    PickedUp,
    CancelPending,
    Cancelled,
    RevisionPending,
}

impl ToSql<crate::schema::sql_types::TicketState, Pg> for TicketState {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        match *self {
            TicketState::CreatePending => out.write_all(b"CREATE_PENDING")?,
            TicketState::AwaitingAcceptance => out.write_all(b"AWAITING_ACCEPTANCE")?,
            TicketState::Accepted => out.write_all(b"ACCEPTED")?,
            TicketState::Preparing => out.write_all(b"PREPARING")?,
            TicketState::ReadyForPickup => out.write_all(b"READY_FOR_PICKUP")?,
            TicketState::PickedUp => out.write_all(b"PICKED_UP")?,
            TicketState::CancelPending => out.write_all(b"CANCEL_PENDING")?,
            TicketState::Cancelled => out.write_all(b"CANCELLED")?,
            TicketState::RevisionPending => out.write_all(b"REVISION_PENDING")?,
        }
        Ok(IsNull::No)
    }
}

impl FromSql<crate::schema::sql_types::TicketState, Pg> for TicketState {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"CREATE_PENDING" => Ok(TicketState::CreatePending),
            b"AWAITING_ACCEPTANCE" => Ok(TicketState::AwaitingAcceptance),
            b"ACCEPTED" => Ok(TicketState::Accepted),
            b"PREPARING" => Ok(TicketState::Preparing),
            b"READY_FOR_PICKUP" => Ok(TicketState::ReadyForPickup),
            b"PICKED_UP" => Ok(TicketState::PickedUp),
            b"CANCEL_PENDING" => Ok(TicketState::CancelPending),
            b"CANCELLED" => Ok(TicketState::Cancelled),
            b"REVISION_PENDING" => Ok(TicketState::RevisionPending),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

impl From<ftgo_proto::kitchen_service::TicketState> for TicketState {
    fn from(s: ftgo_proto::kitchen_service::TicketState) -> Self {
        match s {
            ftgo_proto::kitchen_service::TicketState::CreatePending => TicketState::CreatePending,
            ftgo_proto::kitchen_service::TicketState::AwaitingAcceptance => {
                TicketState::AwaitingAcceptance
            }
            ftgo_proto::kitchen_service::TicketState::Accepted => TicketState::Accepted,
            ftgo_proto::kitchen_service::TicketState::Preparing => TicketState::Preparing,
            ftgo_proto::kitchen_service::TicketState::ReadyForPickup => TicketState::ReadyForPickup,
            ftgo_proto::kitchen_service::TicketState::PickedUp => TicketState::PickedUp,
            ftgo_proto::kitchen_service::TicketState::CancelPending => TicketState::CancelPending,
            ftgo_proto::kitchen_service::TicketState::Cancelled => TicketState::Cancelled,
            ftgo_proto::kitchen_service::TicketState::RevisionPending => {
                TicketState::RevisionPending
            }
        }
    }
}

impl From<TicketState> for ftgo_proto::kitchen_service::TicketState {
    fn from(s: TicketState) -> Self {
        match s {
            TicketState::CreatePending => ftgo_proto::kitchen_service::TicketState::CreatePending,
            TicketState::AwaitingAcceptance => {
                ftgo_proto::kitchen_service::TicketState::AwaitingAcceptance
            }
            TicketState::Accepted => ftgo_proto::kitchen_service::TicketState::Accepted,
            TicketState::Preparing => ftgo_proto::kitchen_service::TicketState::Preparing,
            TicketState::ReadyForPickup => ftgo_proto::kitchen_service::TicketState::ReadyForPickup,
            TicketState::PickedUp => ftgo_proto::kitchen_service::TicketState::PickedUp,
            TicketState::CancelPending => ftgo_proto::kitchen_service::TicketState::CancelPending,
            TicketState::Cancelled => ftgo_proto::kitchen_service::TicketState::Cancelled,
            TicketState::RevisionPending => {
                ftgo_proto::kitchen_service::TicketState::RevisionPending
            }
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = restaurants)]
pub struct Restaurant {
    pub id: Uuid,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(Restaurant))]
#[diesel(table_name = restaurant_menu_items)]
pub struct RestaurantMenuItem {
    pub restaurant_id: Uuid,
    pub id: String,
    pub name: String,
    pub price: BigDecimal,
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = tickets)]
pub struct Ticket {
    pub id: Uuid,
    pub state: TicketState,
    pub previous_state: Option<TicketState>,
    pub restaurant_id: Uuid,
    pub sequence: i64,
    pub ready_by: Option<DateTime<Utc>>,
    pub accept_time: Option<DateTime<Utc>>,
    pub preparing_time: Option<DateTime<Utc>>,
    pub picked_up_time: Option<DateTime<Utc>>,
    pub ready_for_pickup_time: Option<DateTime<Utc>>,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(Ticket))]
#[diesel(table_name = ticket_line_items)]
pub struct TicketLineItem {
    pub ticket_id: Uuid,
    pub id: Uuid,
    pub quantity: i32,
    pub menu_item_id: String,
    pub name: String,
}

#[derive(Queryable, Selectable, Debug, PartialEq)]
#[diesel(table_name = outbox)]
pub struct Outbox {
    pub id: i32,
    pub topic: String,
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Insertable, Debug, PartialEq)]
#[diesel(table_name = outbox)]
pub struct NewOutbox {
    pub topic: String,
    pub key: String,
    pub value: Vec<u8>,
}
