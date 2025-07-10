// @generated automatically by Diesel CLI.

diesel::table! {
    account_details (id) {
        id -> Uuid,
        amount -> Numeric,
        version -> Int8,
        last_processed_sequence -> Int8,
    }
}

diesel::table! {
    account_infos (id) {
        id -> Uuid,
        deposit_accumulate -> Numeric,
        deposit_count -> Int4,
        withdraw_accumulate -> Numeric,
        withdraw_count -> Int4,
        last_processed_sequence -> Int8,
    }
}

diesel::table! {
    checkpoints (subscription_id, stream_name) {
        subscription_id -> Text,
        stream_name -> Text,
        sequence -> Int8,
        checkpointed_at -> Timestamptz,
    }
}

diesel::table! {
    event_stream (name) {
        name -> Text,
        last_sequence -> Int8,
    }
}

diesel::table! {
    events (stream_name, id) {
        stream_name -> Text,
        id -> Uuid,
        sequence -> Int8,
        payload -> Bytea,
        metadata -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::joinable!(checkpoints -> event_stream (stream_name));
diesel::joinable!(events -> event_stream (stream_name));

diesel::allow_tables_to_appear_in_same_query!(
    account_details,
    account_infos,
    checkpoints,
    event_stream,
    events,
);
