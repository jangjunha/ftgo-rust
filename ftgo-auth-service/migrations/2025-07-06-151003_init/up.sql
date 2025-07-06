CREATE TABLE users (
    id          uuid        not null primary key,
    username    text        not null unique,
    created_at  timestamptz not null default now()
);

CREATE TYPE credential_type AS ENUM ('PASSPHRASE');

CREATE TABLE user_credentials (
    user_id         uuid            not null references users(id),
    credential_type credential_type not null,
    sub             text            not null,
    primary key (user_id, credential_type)
);

CREATE TABLE user_restaurant_grants (
    user_id         uuid    not null references users(id),
    restaurant_id   uuid    not null,
    primary key (user_id, restaurant_id)
);

CREATE TABLE user_consumer_grants (
    user_id         uuid    not null references users(id),
    consumer_id   uuid    not null,
    primary key (user_id, consumer_id)
);

CREATE TABLE user_courier_grants (
    user_id         uuid    not null references users(id),
    courier_id   uuid    not null,
    primary key (user_id, courier_id)
);
