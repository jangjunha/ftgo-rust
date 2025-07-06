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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{
    order_line_items, orders, outbox, restaurant_menu_items, restaurants, saga_instances,
};

#[derive(FromSqlRow, AsExpression, PartialEq, Copy, Clone, Debug)]
#[diesel(sql_type = crate::schema::sql_types::OrderState)]
pub enum OrderState {
    ApprovalPending,
    Approved,
    Rejected,
    CancelPending,
    Cancelled,
    RevisionPending,
}

impl ToSql<crate::schema::sql_types::OrderState, Pg> for OrderState {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        match *self {
            OrderState::ApprovalPending => out.write_all(b"APPROVAL_PENDING")?,
            OrderState::Approved => out.write_all(b"APPROVED")?,
            OrderState::Rejected => out.write_all(b"REJECTED")?,
            OrderState::CancelPending => out.write_all(b"CANCEL_PENDING")?,
            OrderState::Cancelled => out.write_all(b"CANCELLED")?,
            OrderState::RevisionPending => out.write_all(b"REVISION_PENDING")?,
        }
        Ok(IsNull::No)
    }
}

impl FromSql<crate::schema::sql_types::OrderState, Pg> for OrderState {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"APPROVAL_PENDING" => Ok(OrderState::ApprovalPending),
            b"APPROVED" => Ok(OrderState::Approved),
            b"REJECTED" => Ok(OrderState::Rejected),
            b"CANCEL_PENDING" => Ok(OrderState::CancelPending),
            b"CANCELLED" => Ok(OrderState::Cancelled),
            b"REVISION_PENDING" => Ok(OrderState::RevisionPending),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

impl From<ftgo_proto::order_service::OrderState> for OrderState {
    fn from(s: ftgo_proto::order_service::OrderState) -> Self {
        match s {
            ftgo_proto::order_service::OrderState::ApprovalPending => OrderState::ApprovalPending,
            ftgo_proto::order_service::OrderState::Approved => OrderState::Approved,
            ftgo_proto::order_service::OrderState::Rejected => OrderState::Rejected,
            ftgo_proto::order_service::OrderState::CancelPending => OrderState::CancelPending,
            ftgo_proto::order_service::OrderState::Cancelled => OrderState::Cancelled,
            ftgo_proto::order_service::OrderState::RevisionPending => OrderState::RevisionPending,
        }
    }
}

impl From<OrderState> for ftgo_proto::order_service::OrderState {
    fn from(s: OrderState) -> Self {
        match s {
            OrderState::ApprovalPending => ftgo_proto::order_service::OrderState::ApprovalPending,
            OrderState::Approved => ftgo_proto::order_service::OrderState::Approved,
            OrderState::Rejected => ftgo_proto::order_service::OrderState::Rejected,
            OrderState::CancelPending => ftgo_proto::order_service::OrderState::CancelPending,
            OrderState::Cancelled => ftgo_proto::order_service::OrderState::Cancelled,
            OrderState::RevisionPending => ftgo_proto::order_service::OrderState::RevisionPending,
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = orders)]
pub struct Order {
    pub id: Uuid,
    pub version: i64,
    pub state: OrderState,
    pub consumer_id: Uuid,
    pub restaurant_id: Uuid,
    pub delivery_time: DateTime<Utc>,
    pub delivery_address: String,
    pub payment_token: Option<String>,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(Order))]
#[diesel(table_name = order_line_items)]
pub struct OrderLineItem {
    pub id: Uuid,
    pub order_id: Uuid,
    pub quantity: i32,
    pub menu_item_id: String,
    pub name: String,
    pub price: BigDecimal,
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = restaurants)]
pub struct Restaurant {
    pub id: Uuid,
    pub name: String,
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

#[derive(Queryable, Selectable, Identifiable, Insertable, AsChangeset, Debug, PartialEq)]
#[diesel(table_name = saga_instances, primary_key(saga_type, saga_id))]
pub struct SagaInstance {
    pub saga_type: String,
    pub saga_id: String,
    pub currently_executing: i32,
    pub last_request_id: Option<String>,
    pub end_state: bool,
    pub compensating: bool,
    pub failed: bool,
    pub saga_data_json: Value,
}
