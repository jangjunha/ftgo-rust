CREATE TABLE saga_instances (
    saga_type           text not null,
    saga_id             text not null,
    currently_executing int not null,
    last_request_id     text,
    end_state           boolean not null,
    compensating        boolean not null,
    failed              boolean not null,
    saga_data_json      jsonb not null,
    PRIMARY KEY (saga_type, saga_id)
);
