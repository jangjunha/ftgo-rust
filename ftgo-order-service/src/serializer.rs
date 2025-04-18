use bigdecimal::BigDecimal;
use ftgo_proto::{
    common::Money,
    order_service::{OrderDetails, OrderLineItem},
};

use crate::models;

pub fn serialize_order_details(
    order: &models::Order,
    line_items: &Vec<models::OrderLineItem>,
    restaurant: &models::Restaurant,
) -> OrderDetails {
    OrderDetails {
        line_items: line_items
            .iter()
            .map(|i| OrderLineItem {
                quantity: i.quantity,
                menu_item_id: i.menu_item_id.clone(),
                name: i.name.clone(),
                price: Some(Money {
                    amount: i.price.to_string(),
                }),
            })
            .collect(),
        order_total: Some(Money {
            amount: line_items
                .iter()
                .map(|i| i.price.clone())
                .sum::<BigDecimal>()
                .to_string(),
        }),
        restaurant_id: restaurant.id.to_string(),
        consumer_id: order.consumer_id.to_string(),
    }
}
