CREATE TABLE checkpoints (
    subscription_id text        not null,
    stream_name     text        not null references event_stream(name),
    sequence        bigint      not null,
    checkpointed_at timestamptz not null,
    primary key (subscription_id, stream_name)
);
