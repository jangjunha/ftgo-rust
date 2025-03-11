CREATE TABLE outbox (
    id          serial      primary key,
    topic       text        not null,
    key         text        not null,
    value       bytea       not null,
    created_at  timestamptz not null default now()
);
