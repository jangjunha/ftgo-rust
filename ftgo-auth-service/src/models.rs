use std::io::Write;

use chrono::{DateTime, Utc};
use diesel::{
    deserialize::FromSql, deserialize::FromSqlRow, expression::AsExpression, prelude::*,
    serialize::ToSql,
};
use uuid::Uuid;

use crate::schema::{
    user_consumer_grants, user_courier_grants, user_credentials, user_restaurant_grants, users,
};

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = users)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(table_name = user_credentials, primary_key(user_id, credential_type))]
pub struct UserCredentials {
    pub user_id: Uuid,
    pub credential_type: CredentialType,
    pub sub: String,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(table_name = user_restaurant_grants, primary_key(user_id, restaurant_id))]
pub struct UserRestaurantGrants {
    pub user_id: Uuid,
    pub restaurant_id: Uuid,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(table_name = user_consumer_grants, primary_key(user_id, consumer_id))]
pub struct UserConsumerGrants {
    pub user_id: Uuid,
    pub consumer_id: Uuid,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(User))]
#[diesel(table_name = user_courier_grants, primary_key(user_id, courier_id))]
pub struct UserCourierGrants {
    pub user_id: Uuid,
    pub courier_id: Uuid,
}

#[derive(FromSqlRow, AsExpression, PartialEq, Eq, Hash, Copy, Clone, Debug)]
#[diesel(sql_type = crate::schema::sql_types::CredentialType)]
pub enum CredentialType {
    Passphrase,
}

impl ToSql<crate::schema::sql_types::CredentialType, diesel::pg::Pg> for CredentialType {
    fn to_sql<'b>(
        &'b self,
        out: &mut diesel::serialize::Output<'b, '_, diesel::pg::Pg>,
    ) -> diesel::serialize::Result {
        match *self {
            CredentialType::Passphrase => out.write_all(b"PASSPHRASE")?,
        }
        Ok(diesel::serialize::IsNull::No)
    }
}

impl FromSql<crate::schema::sql_types::CredentialType, diesel::pg::Pg> for CredentialType {
    fn from_sql(bytes: diesel::pg::PgValue<'_>) -> diesel::deserialize::Result<Self> {
        match bytes.as_bytes() {
            b"PASSPHRASE" => Ok(CredentialType::Passphrase),
            _ => Err("Unrecognized enum variant".into()),
        }
    }
}
