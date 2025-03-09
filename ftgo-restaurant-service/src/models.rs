use bigdecimal::BigDecimal;
use diesel::prelude::*;
use uuid::Uuid;

use crate::schema::{restaurant_menu_items, restaurants};

#[derive(Queryable, Selectable, Identifiable, Insertable, Debug, PartialEq)]
#[diesel(table_name = restaurants)]
pub struct Restaurant {
    pub id: Uuid,
    pub name: String,
    pub address: String,
}

#[derive(Queryable, Selectable, Identifiable, Associations, Insertable, Debug, PartialEq)]
#[diesel(belongs_to(Restaurant))]
#[diesel(table_name = restaurant_menu_items)]
pub struct RestaurantMenuItem {
    pub restaurant_id: Uuid,
    pub id: String,
    pub name: String,
    pub price: BigDecimal,
}
