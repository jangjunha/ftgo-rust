// @generated automatically by Diesel CLI.

diesel::table! {
    consumers (id) {
        id -> Uuid,
        name -> Text,
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

diesel::allow_tables_to_appear_in_same_query!(consumers, outbox,);
