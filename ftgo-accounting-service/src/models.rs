use chrono::{DateTime, Utc};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{checkpoints, event_stream, events};

#[derive(Queryable, Selectable, Identifiable, Insertable, AsChangeset, Debug, PartialEq)]
#[diesel(table_name = event_stream, primary_key(name))]
pub struct EventStream {
    pub name: String,
    pub last_sequence: i64,
}

#[derive(Queryable, Selectable, Identifiable, Debug, PartialEq)]
#[diesel(table_name = events, primary_key(stream_name, id))]
pub struct Event {
    pub stream_name: String,
    pub id: Uuid,
    pub sequence: i64,
    pub payload: Vec<u8>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable, Debug, PartialEq)]
#[diesel(table_name = events)]
pub struct NewEvent {
    pub stream_name: String,
    pub id: Option<Uuid>,
    pub sequence: i64,
    pub payload: Vec<u8>,
    pub metadata: Value,
}

#[derive(
    Queryable,
    Selectable,
    Identifiable,
    Insertable,
    AsChangeset,
    Serialize,
    Deserialize,
    PartialEq,
    Debug,
)]
#[diesel(table_name = checkpoints, primary_key(stream_name, subscription_id))]
pub struct Checkpoint {
    pub stream_name: String,
    pub subscription_id: String,
    pub sequence: i64,
    pub checkpointed_at: DateTime<Utc>,
}
