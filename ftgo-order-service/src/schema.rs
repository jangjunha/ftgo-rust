// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "order_state"))]
    pub struct OrderState;
}

diesel::table! {
    order_line_items (id) {
        id -> Uuid,
        order_id -> Uuid,
        quantity -> Int4,
        menu_item_id -> Text,
        name -> Text,
        price -> Numeric,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::OrderState;

    orders (id) {
        id -> Uuid,
        version -> Int8,
        state -> OrderState,
        consumer_id -> Uuid,
        restaurant_id -> Uuid,
        delivery_time -> Timestamptz,
        delivery_address -> Text,
        payment_token -> Nullable<Text>,
        created_at -> Timestamptz,
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
    restaurant_menu_items (restaurant_id, id) {
        restaurant_id -> Uuid,
        id -> Text,
        name -> Text,
        price -> Numeric,
    }
}

diesel::table! {
    restaurants (id) {
        id -> Uuid,
        name -> Text,
    }
}

diesel::table! {
    saga_instances (saga_type, saga_id) {
        saga_type -> Text,
        saga_id -> Text,
        currently_executing -> Int4,
        last_request_id -> Nullable<Text>,
        end_state -> Bool,
        compensating -> Bool,
        failed -> Bool,
        saga_data_json -> Jsonb,
    }
}

diesel::joinable!(order_line_items -> orders (order_id));
diesel::joinable!(orders -> restaurants (restaurant_id));
diesel::joinable!(restaurant_menu_items -> restaurants (restaurant_id));

diesel::allow_tables_to_appear_in_same_query!(
    order_line_items,
    orders,
    outbox,
    restaurant_menu_items,
    restaurants,
    saga_instances,
);
