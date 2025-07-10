CREATE TABLE event_stream(
    name            text        not null primary key,
    last_sequence   bigint      not null
);

CREATE TABLE events (
    stream_name text        not null references event_stream(name),
    id          uuid        not null default gen_random_uuid(),
    sequence    bigint      not null,
    payload     bytea       not null,
    metadata    jsonb       not null,
    created_at  timestamptz not null default now(),
    primary key (stream_name, id)
);
CREATE UNIQUE INDEX ix_events_stream_name_sequence ON events (stream_name, sequence);
