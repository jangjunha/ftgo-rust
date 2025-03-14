CREATE TYPE order_state AS ENUM (
    'APPROVAL_PENDING',
    'APPROVED',
    'REJECTED',
    'CANCEL_PENDING',
    'CANCELLED',
    'REVISION_PENDING'
);

CREATE TABLE orders (
    id                  uuid        not null primary key,
    version             bigint      not null,
    state               order_state not null,
    consumer_id         uuid        not null,
    restaurant_id       uuid        not null references restaurants(id),
    delivery_time       timestamptz not null,
    delivery_address    text        not null,
    payment_token       text
);

CREATE TABLE order_line_items (
    id                  uuid        not null primary key,
    order_id            uuid        not null references orders(id),
    quantity            int         not null,
    menu_item_id        text        not null,
    name                text        not null,
    price               numeric     not null
);
