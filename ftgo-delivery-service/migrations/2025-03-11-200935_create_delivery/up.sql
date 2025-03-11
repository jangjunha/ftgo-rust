CREATE TABLE restaurants (
    id      uuid    not null primary key,
    name    text    not null,
    address text    not null
);

CREATE TYPE delivery_state AS ENUM (
    'PENDING',
    'SCHEDULED',
    'CANCELLED'
);

CREATE TABLE deliveries (
    id                  uuid            not null primary key,
    pickup_address      text            not null,
    state               delivery_state  not null,
    restaurant_id       uuid            not null references restaurants(id),
    pickup_time         timestamptz,
    delivery_address    text            not null,
    delivery_time       timestamptz,
    assigned_courier_id uuid            references couriers(id),
    ready_by            timestamptz
);
