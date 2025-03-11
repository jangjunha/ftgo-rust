use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::{consumers, outbox};

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = consumers)]
pub struct Consumer {
    pub id: Uuid,
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
