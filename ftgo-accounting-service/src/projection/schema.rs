// @generated automatically by Diesel CLI.

diesel::table! {
    account_details (id) {
        id -> Uuid,
        amount -> Numeric,
        version -> Int8,
        last_processed_position -> Int8,
    }
}

diesel::table! {
    account_infos (id) {
        id -> Uuid,
        deposit_accumulate -> Numeric,
        deposit_count -> Int4,
        withdraw_accumulate -> Numeric,
        withdraw_count -> Int4,
        last_processed_position -> Int8,
    }
}

diesel::allow_tables_to_appear_in_same_query!(account_details, account_infos,);
