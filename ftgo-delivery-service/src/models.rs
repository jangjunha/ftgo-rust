use std::io::Write;

use chrono::{DateTime, Utc};
use diesel::{
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    pg::{Pg, PgValue},
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
};
use uuid::Uuid;

use crate::schema::{courier_actions, couriers, deliveries, outbox, restaurants};

#[derive(FromSqlRow, AsExpression, PartialEq, Copy, Clone, Debug)]
#[diesel(sql_type = crate::schema::sql_types::DeliveryState)]
pub enum DeliveryState {
    Pending,
    Scheduled,
    Cancelled,
}

impl ToSql<crate::schema::sql_types::DeliveryState, Pg> for DeliveryState {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        match *self {
            DeliveryState::Pending => out.write_all(b"PENDING")?,
            DeliveryState::Scheduled => out.write_all(b"SCHEDULED")?,
            DeliveryState::Cancelled => out.write_all(b"CANCELLED")?,
        }
        Ok(IsNull::No)
    }
}

impl FromSql<crate::schema::sql_types::DeliveryState, Pg> for DeliveryState {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"PENDING" => Ok(DeliveryState::Pending),
            b"SCHEDULED" => Ok(DeliveryState::Scheduled),
            b"CANCELLED" => Ok(DeliveryState::Cancelled),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

impl From<ftgo_proto::delivery_service::DeliveryState> for DeliveryState {
    fn from(s: ftgo_proto::delivery_service::DeliveryState) -> Self {
        match s {
            ftgo_proto::delivery_service::DeliveryState::Pending => DeliveryState::Pending,
            ftgo_proto::delivery_service::DeliveryState::Scheduled => DeliveryState::Scheduled,
            ftgo_proto::delivery_service::DeliveryState::Cancelled => DeliveryState::Cancelled,
        }
    }
}

impl From<DeliveryState> for ftgo_proto::delivery_service::DeliveryState {
    fn from(s: DeliveryState) -> Self {
        match s {
            DeliveryState::Pending => ftgo_proto::delivery_service::DeliveryState::Pending,
            DeliveryState::Scheduled => ftgo_proto::delivery_service::DeliveryState::Scheduled,
            DeliveryState::Cancelled => ftgo_proto::delivery_service::DeliveryState::Cancelled,
        }
    }
}

#[derive(FromSqlRow, AsExpression, PartialEq, Copy, Clone, Debug)]
#[diesel(sql_type = crate::schema::sql_types::DeliveryActionType)]
pub enum DeliveryActionType {
    Pickup,
    Dropoff,
}

impl ToSql<crate::schema::sql_types::DeliveryActionType, Pg> for DeliveryActionType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> serialize::Result {
        match *self {
            DeliveryActionType::Pickup => out.write_all(b"PICKUP")?,
            DeliveryActionType::Dropoff => out.write_all(b"DROPOFF")?,
        }
        Ok(IsNull::No)
    }
}

impl FromSql<crate::schema::sql_types::DeliveryActionType, Pg> for DeliveryActionType {
    fn from_sql(bytes: PgValue<'_>) -> deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"PICKUP" => Ok(DeliveryActionType::Pickup),
            b"DROPOFF" => Ok(DeliveryActionType::Dropoff),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}

impl From<ftgo_proto::delivery_service::DeliveryActionType> for DeliveryActionType {
    fn from(s: ftgo_proto::delivery_service::DeliveryActionType) -> Self {
        match s {
            ftgo_proto::delivery_service::DeliveryActionType::Pickup => DeliveryActionType::Pickup,
            ftgo_proto::delivery_service::DeliveryActionType::Dropoff => {
                DeliveryActionType::Dropoff
            }
        }
    }
}

impl From<DeliveryActionType> for ftgo_proto::delivery_service::DeliveryActionType {
    fn from(s: DeliveryActionType) -> Self {
        match s {
            DeliveryActionType::Pickup => ftgo_proto::delivery_service::DeliveryActionType::Pickup,
            DeliveryActionType::Dropoff => {
                ftgo_proto::delivery_service::DeliveryActionType::Dropoff
            }
        }
    }
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = couriers)]
pub struct Courier {
    pub id: Uuid,
    pub available: bool,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, PartialEq)]
#[diesel(belongs_to(Courier))]
#[diesel(table_name = courier_actions)]
pub struct CourierAction {
    pub id: i32,
    pub courier_id: Uuid,
    pub type_: DeliveryActionType,
    pub delivery_id: Uuid,
    pub address: String,
    pub time: DateTime<Utc>,
}

#[derive(Insertable, Debug, PartialEq)]
#[diesel(belongs_to(Courier))]
#[diesel(table_name = courier_actions)]
pub struct NewCourierAction {
    pub courier_id: Uuid,
    pub type_: DeliveryActionType,
    pub delivery_id: Uuid,
    pub address: String,
    pub time: DateTime<Utc>,
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = deliveries)]
pub struct Delivery {
    pub id: Uuid,
    pub pickup_address: String,
    pub state: DeliveryState,
    pub restaurant_id: Uuid,
    pub pickup_time: Option<DateTime<Utc>>,
    pub delivery_address: String,
    pub delivery_time: Option<DateTime<Utc>>,
    pub assigned_courier_id: Option<Uuid>,
    pub ready_by: Option<DateTime<Utc>>,
}

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = restaurants)]
pub struct Restaurant {
    pub id: Uuid,
    pub name: String,
    pub address: String,
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
