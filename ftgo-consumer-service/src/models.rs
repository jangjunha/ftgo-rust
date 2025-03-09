use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::consumers;

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = consumers)]
pub struct Consumer {
    pub id: Uuid,
    pub name: String,
}
