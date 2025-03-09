CREATE TABLE restaurants (
  id      uuid  not null primary key,
  name    text  not null,
  address text  not null
);

CREATE TABLE restaurant_menu_items (
  restaurant_id uuid    not null references restaurants(id),
  id            text    not null,
  name          text    not null,
  price         decimal not null,
  primary key (restaurant_id, id)
);
