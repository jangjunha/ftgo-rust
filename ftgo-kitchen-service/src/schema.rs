// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::query_builder::QueryId, Clone, diesel::sql_types::SqlType)]
    #[diesel(postgres_type(name = "ticket_state"))]
    pub struct TicketState;
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
    }
}

diesel::table! {
    ticket_line_items (ticket_id, id) {
        ticket_id -> Uuid,
        id -> Uuid,
        quantity -> Int4,
        menu_item_id -> Text,
        name -> Text,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::TicketState;

    tickets (id) {
        id -> Uuid,
        state -> TicketState,
        previous_state -> Nullable<TicketState>,
        restaurant_id -> Uuid,
        sequence -> Int8,
        ready_by -> Nullable<Timestamptz>,
        accept_time -> Nullable<Timestamptz>,
        preparing_time -> Nullable<Timestamptz>,
        picked_up_time -> Nullable<Timestamptz>,
        ready_for_pickup_time -> Nullable<Timestamptz>,
    }
}

diesel::joinable!(restaurant_menu_items -> restaurants (restaurant_id));
diesel::joinable!(ticket_line_items -> tickets (ticket_id));

diesel::allow_tables_to_appear_in_same_query!(
    outbox,
    restaurant_menu_items,
    restaurants,
    ticket_line_items,
    tickets,
);
