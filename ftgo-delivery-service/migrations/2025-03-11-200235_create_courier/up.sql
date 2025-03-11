CREATE TABLE couriers (
    id          uuid    not null primary key,
    available   bool    not null default false
);
