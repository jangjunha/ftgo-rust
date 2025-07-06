use bigdecimal::BigDecimal;
use ftgo_proto::{
    common::Money,
    kitchen_service::TicketLineItem,
    order_service::{OrderDetails, OrderLineItem},
};

use crate::models;

impl Into<OrderLineItem> for &models::OrderLineItem {
    fn into(self) -> OrderLineItem {
        OrderLineItem {
            quantity: self.quantity,
            menu_item_id: self.menu_item_id.clone(),
            name: self.name.clone(),
            price: Some(Money {
                amount: self.price.to_string(),
            }),
        }
    }
}

impl Into<TicketLineItem> for &models::OrderLineItem {
    fn into(self) -> TicketLineItem {
        TicketLineItem {
            quantity: self.quantity,
            menu_item_id: self.menu_item_id.clone(),
            name: self.name.clone(),
        }
    }
}

pub fn serialize_order_details(
    order: &models::Order,
    line_items: &Vec<models::OrderLineItem>,
    restaurant: &models::Restaurant,
) -> OrderDetails {
    OrderDetails {
        line_items: line_items.iter().map(|i| i.into()).collect(),
        order_total: Some(Money {
            amount: line_items
                .iter()
                .map(|i| i.total_price())
                .sum::<BigDecimal>()
                .to_string(),
        }),
        restaurant_id: restaurant.id.to_string(),
        consumer_id: order.consumer_id.to_string(),
    }
}
