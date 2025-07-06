use crate::{
    models::OrderLineItem,
    proxy::{
        accounting_service::AccountingServiceProxy, consumer_service::ConsumerServiceProxy,
        kitchen_service::KitchenServiceProxy, order_service::OrderServiceProxy,
    },
};
use diesel::PgConnection;
use ftgo_proto::{
    common::CommandReply,
    kitchen_service::{CreateTicketCommandReply, TicketDetails},
};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use super::{Saga, SagaDefition, SagaStep};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateOrderSagaState {
    pub order_id: Uuid,
    pub line_items: Vec<OrderLineItem>,
    pub restaurant_id: Uuid,
    pub consumer_id: Uuid,
    pub ticket_id: Option<Uuid>,
}

impl CreateOrderSagaState {
    pub fn new(
        order_id: &Uuid,
        line_items: &Vec<OrderLineItem>,
        restaurant_id: &Uuid,
        consumer_id: &Uuid,
    ) -> Self {
        Self {
            order_id: order_id.clone(),
            line_items: line_items.clone(),
            restaurant_id: restaurant_id.clone(),
            consumer_id: consumer_id.clone(),
            ticket_id: None,
        }
    }
}

pub struct CreateOrderSaga<'a> {
    pub saga_definition: SagaDefition<'a, CreateOrderSagaState>,
}

impl<'a> CreateOrderSaga<'a> {
    pub fn new() -> Self {
        let approve_order = |saga_state: &CreateOrderSagaState,
                             saga_headers: &HashMap<String, String>,
                             conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut order_service = OrderServiceProxy::new(conn);
            order_service.approve_order(&saga_state.order_id, &state)?;
            println!(
                "REQUESTED:{}: approve_order order={}",
                request_id, saga_state.order_id
            );
            Ok(request_id)
        };
        let reject_order = |saga_state: &CreateOrderSagaState,
                            saga_headers: &HashMap<String, String>,
                            conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut order_service = OrderServiceProxy::new(conn);
            order_service.reject_order(&saga_state.order_id, &state)?;
            println!(
                "REQUESTED:{}: reject_order order={}",
                request_id, saga_state.order_id
            );
            Ok(request_id)
        };
        let validate_order_by_consumer =
            |saga_state: &CreateOrderSagaState,
             saga_headers: &HashMap<String, String>,
             conn: &mut PgConnection| {
                let request_id = Uuid::new_v4().to_string();
                let state = {
                    let mut state = saga_headers.clone();
                    state.insert("REQUEST-ID".to_string(), request_id.to_string());
                    state
                };
                let mut consumer_service = ConsumerServiceProxy::new(conn);
                consumer_service.validate_order_by_consumer(
                    &saga_state.consumer_id,
                    &saga_state.order_id,
                    &saga_state
                        .line_items
                        .iter()
                        .map(|li| li.total_price())
                        .sum(),
                    &state,
                )?;
                println!(
                    "REQUESTED:{}: validate_order_by_consumer order={} consumer={}",
                    request_id, saga_state.consumer_id, saga_state.order_id
                );
                Ok(request_id)
            };
        let create_ticket = |saga_state: &CreateOrderSagaState,
                             saga_headers: &HashMap<String, String>,
                             conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut kitchen_service = KitchenServiceProxy::new(conn);
            kitchen_service.create_ticket(
                &saga_state.order_id,
                &TicketDetails {
                    line_items: saga_state.line_items.iter().map(|li| li.into()).collect(),
                },
                &saga_state.restaurant_id,
                &state,
            )?;
            println!(
                "REQUESTED:{}: create_ticket order={}",
                request_id, saga_state.order_id
            );
            Ok(request_id)
        };
        let handle_create_ticket = |mut state: CreateOrderSagaState, reply: &CommandReply| {
            let body = CreateTicketCommandReply::decode(
                &reply
                    .body
                    .as_ref()
                    .expect("Create ticket command reply body is empty")[..],
            )
            .expect("Cannot decode create ticket command reply");
            state.ticket_id = Some(Uuid::parse_str(&body.id).unwrap());
            state
        };
        let cancel_create_ticket = |saga_state: &CreateOrderSagaState,
                                    saga_headers: &HashMap<String, String>,
                                    conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut kitchen_service = KitchenServiceProxy::new(conn);
            kitchen_service.cancel_create_ticket(
                &saga_state.order_id,
                &saga_state.restaurant_id,
                &state,
            )?;
            println!(
                "REQUESTED:{}: cancel_create_ticket order={}",
                request_id, saga_state.order_id
            );
            Ok(request_id)
        };
        let confirm_create_ticket = |saga_state: &CreateOrderSagaState,
                                     saga_headers: &HashMap<String, String>,
                                     conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut kitchen_service = KitchenServiceProxy::new(conn);
            kitchen_service.confirm_create_ticket(
                &saga_state.order_id,
                &saga_state.restaurant_id,
                &state,
            )?;
            println!(
                "REQUESTED:{}: confirm_create_ticket order={}",
                request_id, saga_state.order_id
            );
            Ok(request_id)
        };
        let withdraw = |saga_state: &CreateOrderSagaState,
                        saga_headers: &HashMap<String, String>,
                        conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut accounting_service = AccountingServiceProxy::new(conn);
            accounting_service.withdraw(
                &saga_state.consumer_id,
                &saga_state.order_id,
                &saga_state
                    .line_items
                    .iter()
                    .map(|li| li.total_price())
                    .sum(),
                &state,
            )?;
            println!(
                "REQUESTED:{}: withdraw order={} account={}",
                request_id, saga_state.order_id, saga_state.consumer_id,
            );
            Ok(request_id)
        };
        let deposit = |saga_state: &CreateOrderSagaState,
                       saga_headers: &HashMap<String, String>,
                       conn: &mut PgConnection| {
            let request_id = Uuid::new_v4().to_string();
            let state = {
                let mut state = saga_headers.clone();
                state.insert("REQUEST-ID".to_string(), request_id.to_string());
                state
            };
            let mut accounting_service = AccountingServiceProxy::new(conn);
            accounting_service.deposit(
                &saga_state.consumer_id,
                &saga_state.order_id,
                &saga_state
                    .line_items
                    .iter()
                    .map(|li| li.total_price())
                    .sum(),
                &state,
            )?;
            println!(
                "REQUESTED:{}: deposit order={} account={}",
                request_id, saga_state.order_id, saga_state.consumer_id,
            );
            Ok(request_id)
        };

        Self {
            saga_definition: SagaDefition {
                steps: vec![
                    SagaStep {
                        invoke: None,
                        on_reply: None,
                        invoke_compensation: Some(Box::new(reject_order)),
                    },
                    SagaStep {
                        invoke: Some(Box::new(validate_order_by_consumer)),
                        on_reply: None,
                        invoke_compensation: None,
                    },
                    SagaStep {
                        invoke: Some(Box::new(create_ticket)),
                        on_reply: Some(Box::new(handle_create_ticket)),
                        invoke_compensation: Some(Box::new(cancel_create_ticket)),
                    },
                    SagaStep {
                        invoke: Some(Box::new(withdraw)),
                        on_reply: None,
                        invoke_compensation: Some(Box::new(deposit)),
                    },
                    SagaStep {
                        invoke: Some(Box::new(confirm_create_ticket)),
                        on_reply: None,
                        invoke_compensation: None,
                    },
                    SagaStep {
                        invoke: Some(Box::new(approve_order)),
                        on_reply: None,
                        invoke_compensation: None,
                    },
                ],
            },
        }
    }
}

impl<'a> Saga<CreateOrderSagaState> for CreateOrderSaga<'a> {
    fn r#type(&self) -> &'static str {
        "create-order"
    }

    fn get_definition(&self) -> &SagaDefition<'a, CreateOrderSagaState> {
        &self.saga_definition
    }
}
