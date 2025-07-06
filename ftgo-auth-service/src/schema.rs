// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "credential_type"))]
    pub struct CredentialType;
}

diesel::table! {
    user_consumer_grants (user_id, consumer_id) {
        user_id -> Uuid,
        consumer_id -> Uuid,
    }
}

diesel::table! {
    user_courier_grants (user_id, courier_id) {
        user_id -> Uuid,
        courier_id -> Uuid,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::CredentialType;

    user_credentials (user_id, credential_type) {
        user_id -> Uuid,
        credential_type -> CredentialType,
        sub -> Text,
    }
}

diesel::table! {
    user_restaurant_grants (user_id, restaurant_id) {
        user_id -> Uuid,
        restaurant_id -> Uuid,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        username -> Text,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(user_consumer_grants -> users (user_id));
diesel::joinable!(user_courier_grants -> users (user_id));
diesel::joinable!(user_credentials -> users (user_id));
diesel::joinable!(user_restaurant_grants -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    user_consumer_grants,
    user_courier_grants,
    user_credentials,
    user_restaurant_grants,
    users,
);
