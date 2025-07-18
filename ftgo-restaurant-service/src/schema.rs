// @generated automatically by Diesel CLI.

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
        address -> Text,
    }
}

diesel::joinable!(restaurant_menu_items -> restaurants (restaurant_id));

diesel::allow_tables_to_appear_in_same_query!(outbox, restaurant_menu_items, restaurants,);
