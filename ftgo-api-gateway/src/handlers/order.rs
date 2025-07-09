use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
};
use ftgo_proto::order_service::{
    CreateOrderPayload, GetOrderPayload, ListOrderPayload, MenuItemIdAndQuantity,
};
use serde::Deserialize;
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, extract_user_id_from_token, verify_consumer_access, verify_order_access};

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub consumer_id: Option<String>,
    pub restaurant_id: Option<String>,
    pub state: Option<String>,
    pub first: Option<u32>,
    pub after: Option<String>,
    pub last: Option<u32>,
    pub before: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders", post(create_order).get(list_orders))
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
        consumer_id: order
            .consumer_id
            .parse()
            .map_err(|_| ApiError::InvalidToken)?,
        restaurant_id: order
            .restaurant_id
            .parse()
            .map_err(|_| ApiError::InvalidToken)?,
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
        consumer_id: order
            .consumer_id
            .parse()
            .map_err(|_| ApiError::InvalidToken)?,
        restaurant_id: order
            .restaurant_id
            .parse()
            .map_err(|_| ApiError::InvalidToken)?,
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
    path = "/orders",
    responses(
        (status = 200, description = "List of orders", body = ListOrdersResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("consumer_id" = Option<String>, Query, description = "Filter by consumer ID"),
        ("restaurant_id" = Option<String>, Query, description = "Filter by restaurant ID"),
        ("state" = Option<String>, Query, description = "Filter by order state"),
        ("first" = Option<u32>, Query, description = "Number of orders to fetch"),
        ("after" = Option<String>, Query, description = "Cursor for pagination"),
        ("last" = Option<u32>, Query, description = "Number of orders to fetch from end"),
        ("before" = Option<String>, Query, description = "Cursor for pagination"),
    ),
    security(
        ("bearer" = [])
    ),
    tag = "orders"
)]
#[instrument(skip(state))]
pub async fn list_orders(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListOrdersQuery>,
) -> Result<Json<ListOrdersResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Extract user ID from token
    let user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    // Get user profile to check permissions
    let user_profile_request = tonic::Request::new(ftgo_proto::auth_service::GetUserPayload {
        id: user_id.clone(),
    });

    let user_profile_response = auth_client
        .get_user(user_profile_request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user_profile = user_profile_response.into_inner();

    let mut order_client = state.order_client.clone();

    // Parse order state if provided
    let order_state = if let Some(state_str) = query.state {
        Some(match state_str.as_str() {
            "APPROVAL_PENDING" => 0,
            "APPROVED" => 1,
            "REJECTED" => 2,
            "CANCEL_PENDING" => 3,
            "CANCELLED" => 4,
            "REVISION_PENDING" => 5,
            _ => return Err(ApiError::InvalidToken),
        })
    } else {
        None
    };

    // If specific consumer_id or restaurant_id is requested, verify access
    if let Some(consumer_id) = &query.consumer_id {
        if !user_profile.granted_consumers.contains(consumer_id) {
            return Err(ApiError::Forbidden);
        }
    }

    if let Some(restaurant_id) = &query.restaurant_id {
        if !user_profile.granted_restaurants.contains(restaurant_id) {
            return Err(ApiError::Forbidden);
        }
    }

    // Build the request payload
    let request = tonic::Request::new(ListOrderPayload {
        consumer_id: query.consumer_id.clone(),
        restaurant_id: query.restaurant_id.clone(),
        state: order_state,
        first: query.first,
        after: query.after.clone(),
        last: query.last,
        before: query.before.clone(),
    });

    let response = order_client
        .list_order(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Order service error: {e}")))?;

    let orders_response = response.into_inner();

    // Filter orders based on user permissions
    let filtered_edges: Vec<_> = orders_response
        .edges
        .into_iter()
        .filter_map(|edge| {
            let order = edge.node.as_ref()?;

            // Check if user has access to this order (consumer or restaurant)
            let has_consumer_access = user_profile.granted_consumers.contains(&order.consumer_id);
            let has_restaurant_access = user_profile
                .granted_restaurants
                .contains(&order.restaurant_id);

            if has_consumer_access || has_restaurant_access {
                Some(edge)
            } else {
                None
            }
        })
        .collect();

    // Convert to API response format
    let edges = filtered_edges
        .into_iter()
        .map(|edge| -> Result<crate::models::OrderEdge, ApiError> {
            let order = edge.node.unwrap();

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

            Ok(crate::models::OrderEdge {
                node: CreateOrderResponse {
                    id: order.id.parse().map_err(|_| ApiError::InvalidToken)?,
                    state: state_str.to_string(),
                    consumer_id: order
                        .consumer_id
                        .parse()
                        .map_err(|_| ApiError::InvalidToken)?,
                    restaurant_id: order
                        .restaurant_id
                        .parse()
                        .map_err(|_| ApiError::InvalidToken)?,
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
                            .and_then(|ts| {
                                chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
                            }),
                        delivery_address: order
                            .delivery_information
                            .map(|di| di.delivery_address)
                            .unwrap_or_default(),
                    },
                    order_minimum: order.order_minimum.map(|m| m.amount).unwrap_or_default(),
                },
                cursor: edge.cursor,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ListOrdersResponse { edges }))
}
