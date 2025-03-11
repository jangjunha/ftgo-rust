CREATE TYPE delivery_action_type AS ENUM (
    'PICKUP',
    'DROPOFF'
);

CREATE TABLE courier_actions (
    id          serial                  not null primary key,
    courier_id  uuid                    not null references couriers(id),
    type        delivery_action_type    not null,
    delivery_id uuid                    not null references deliveries(id),
    address     text                    not null,
    time        timestamptz             not null
);
