use axum::{
    Router,
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
};
use ftgo_proto::order_service::{CreateOrderPayload, GetOrderPayload, MenuItemIdAndQuantity};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, verify_consumer_access, verify_order_access};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(create_order))
        .route("/orders/{id}", get(get_order))
}

#[utoipa::path(
    post,
    path = "/orders",
    request_body = CreateOrderRequest,
    responses(
        (status = 200, description = "Order created successfully", body = CreateOrderResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    security(
        ("bearer" = [])
    ),
    tag = "orders"
)]
#[instrument(skip(state))]
pub async fn create_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateOrderRequest>,
) -> Result<Json<CreateOrderResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to the consumer they're trying to order for
    verify_consumer_access(&headers, &mut auth_client, &payload.consumer_id.to_string()).await?;

    let mut order_client = state.order_client.clone();

    let items = payload
        .items
        .into_iter()
        .map(|item| MenuItemIdAndQuantity {
            menu_item_id: item.menu_item_id,
            quantity: item.quantity,
        })
        .collect();

    let request = tonic::Request::new(CreateOrderPayload {
        restaurant_id: payload.restaurant_id.to_string(),
        consumer_id: payload.consumer_id.to_string(),
        items,
        delivery_address: payload.delivery_address,
    });

    let response = order_client
        .create_order(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Order service error: {e}")))?;

    let order = response.into_inner();

    // Convert the OrderState enum to string
    let state_str = match order.state {
        0 => "APPROVAL_PENDING",
        1 => "APPROVED",
        2 => "REJECTED",
        3 => "CANCEL_PENDING",
        4 => "CANCELLED",
        5 => "REVISION_PENDING",
        _ => "UNKNOWN",
    };

    Ok(Json(CreateOrderResponse {
        id: order.id.parse().map_err(|_| ApiError::InvalidToken)?,
        state: state_str.to_string(),
        consumer_id: order.consumer_id.parse().map_err(|_| ApiError::InvalidToken)?,
        restaurant_id: order.restaurant_id.parse().map_err(|_| ApiError::InvalidToken)?,
        line_items: order
            .line_items
            .into_iter()
            .map(|item| crate::models::OrderLineItem {
                quantity: item.quantity,
                menu_item_id: item.menu_item_id,
                name: item.name,
                price: item.price.map(|p| p.amount).unwrap_or_default(),
            })
            .collect(),
        delivery_information: crate::models::DeliveryInformation {
            delivery_time: order
                .delivery_information
                .as_ref()
                .and_then(|di| di.delivery_time.as_ref())
                .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
            delivery_address: order
                .delivery_information
                .map(|di| di.delivery_address)
                .unwrap_or_default(),
        },
        order_minimum: order.order_minimum.map(|m| m.amount).unwrap_or_default(),
    }))
}

#[utoipa::path(
    get,
    path = "/orders/{id}",
    responses(
        (status = 200, description = "Order details", body = CreateOrderResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Order not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("id" = String, Path, description = "Order ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "orders"
)]
#[instrument(skip(state))]
pub async fn get_order(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<CreateOrderResponse>, ApiError> {
    let mut order_client = state.order_client.clone();

    let request = tonic::Request::new(GetOrderPayload { id: order_id });

    let response = order_client.get_order(request).await.map_err(|e| {
        if e.code() == tonic::Code::NotFound {
            ApiError::ServiceUnavailable("Order not found".to_string())
        } else {
            ApiError::ServiceUnavailable(format!("Order service error: {e}"))
        }
    })?;

    let order = response.into_inner();

    // Verify user has access to either the consumer or restaurant for this order
    let mut auth_client = state.auth_client.clone();
    verify_order_access(
        &headers,
        &mut auth_client,
        &order.consumer_id,
        &order.restaurant_id,
    )
    .await?;

    // Convert the OrderState enum to string
    let state_str = match order.state {
        0 => "APPROVAL_PENDING",
        1 => "APPROVED",
        2 => "REJECTED",
        3 => "CANCEL_PENDING",
        4 => "CANCELLED",
        5 => "REVISION_PENDING",
        _ => "UNKNOWN",
    };

    Ok(Json(CreateOrderResponse {
        id: order.id.parse().map_err(|_| ApiError::InvalidToken)?,
        state: state_str.to_string(),
        consumer_id: order.consumer_id.parse().map_err(|_| ApiError::InvalidToken)?,
        restaurant_id: order.restaurant_id.parse().map_err(|_| ApiError::InvalidToken)?,
        line_items: order
            .line_items
            .into_iter()
            .map(|item| crate::models::OrderLineItem {
                quantity: item.quantity,
                menu_item_id: item.menu_item_id,
                name: item.name,
                price: item.price.map(|p| p.amount).unwrap_or_default(),
            })
            .collect(),
        delivery_information: crate::models::DeliveryInformation {
            delivery_time: order
                .delivery_information
                .as_ref()
                .and_then(|di| di.delivery_time.as_ref())
                .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
            delivery_address: order
                .delivery_information
                .map(|di| di.delivery_address)
                .unwrap_or_default(),
        },
        order_minimum: order.order_minimum.map(|m| m.amount).unwrap_or_default(),
    }))
}
