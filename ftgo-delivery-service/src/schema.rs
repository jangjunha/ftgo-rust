// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "delivery_action_type"))]
    pub struct DeliveryActionType;

    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "delivery_state"))]
    pub struct DeliveryState;
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::DeliveryActionType;

    courier_actions (id) {
        id -> Int4,
        courier_id -> Uuid,
        #[sql_name = "type"]
        type_ -> DeliveryActionType,
        delivery_id -> Uuid,
        address -> Text,
        time -> Timestamptz,
    }
}

diesel::table! {
    couriers (id) {
        id -> Uuid,
        available -> Bool,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::DeliveryState;

    deliveries (id) {
        id -> Uuid,
        pickup_address -> Text,
        state -> DeliveryState,
        restaurant_id -> Uuid,
        pickup_time -> Nullable<Timestamptz>,
        delivery_address -> Text,
        delivery_time -> Nullable<Timestamptz>,
        assigned_courier_id -> Nullable<Uuid>,
        ready_by -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    outbox (id) {
        id -> Int4,
        topic -> Text,
        key -> Text,
        value -> Bytea,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    restaurants (id) {
        id -> Uuid,
        name -> Text,
        address -> Text,
    }
}

diesel::joinable!(courier_actions -> couriers (courier_id));
diesel::joinable!(courier_actions -> deliveries (delivery_id));
diesel::joinable!(deliveries -> couriers (assigned_courier_id));
diesel::joinable!(deliveries -> restaurants (restaurant_id));

diesel::allow_tables_to_appear_in_same_query!(
    courier_actions,
    couriers,
    deliveries,
    outbox,
    restaurants,
);
