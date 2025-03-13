CREATE TABLE account_infos (
    id                      uuid    not null primary key,
    deposit_accumulate      numeric not null,
    deposit_count           int     not null,
    withdraw_accumulate     numeric not null,
    withdraw_count          int     not null,
    last_processed_position bigint  not null
);

CREATE TABLE account_details (
    id                      uuid    not null primary key,
    amount                  numeric not null,
    version                 bigint  not null,
    last_processed_position bigint  not null
);
