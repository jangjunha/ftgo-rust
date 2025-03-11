CREATE TYPE ticket_state AS ENUM (
    'CREATE_PENDING',
    'AWAITING_ACCEPTANCE',
    'ACCEPTED',
    'PREPARING',
    'READY_FOR_PICKUP',
    'PICKED_UP',
    'CANCEL_PENDING',
    'CANCELLED',
    'REVISION_PENDING'
);

CREATE TABLE tickets (
  id                    uuid          not null primary key,
  state                 ticket_state  not null,
  previous_state        ticket_state,
  restaurant_id         uuid          not null,
  sequence              bigint        not null,
  ready_by              timestamptz,
  accept_time           timestamptz,
  preparing_time        timestamptz,
  picked_up_time        timestamptz,
  ready_for_pickup_time timestamptz
);

CREATE TABLE ticket_line_items (
  ticket_id     uuid    not null references tickets(id),
  id            uuid    not null,
  quantity      int     not null,
  menu_item_id  text    not null,
  name          text    not null,
  primary key (ticket_id, id)
);
