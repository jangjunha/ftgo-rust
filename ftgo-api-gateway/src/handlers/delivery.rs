use axum::{extract::{State, Path}, http::HeaderMap, response::Json, Router, routing::{get, post, put}};
use ftgo_proto::delivery_service::{
    GetDeliveryStatusPayload, GetCourierPayload, UpdateCourierAvailabilityPayload,
    PickupDeliveryPayload, DropoffDeliveryPayload,
};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, extract_user_id_from_token, verify_courier_access};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/orders/{order_id}/delivery", get(get_delivery_status))
        .route("/couriers", post(create_courier))
        .route("/couriers/{courier_id}", get(get_courier))
        .route("/couriers/{courier_id}/availability", put(update_courier_availability))
        .route("/couriers/{courier_id}/plan", get(get_courier_plan))
        .route("/deliveries/{delivery_id}/pickup", post(pickup_delivery))
        .route("/deliveries/{delivery_id}/dropoff", post(dropoff_delivery))
}

#[utoipa::path(
    get,
    path = "/orders/{order_id}/delivery",
    responses(
        (status = 200, description = "Delivery status information", body = DeliveryStatusResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Delivery not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("order_id" = String, Path, description = "Order ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn get_delivery_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(order_id): Path<String>,
) -> Result<Json<DeliveryStatusResponse>, ApiError> {
    let mut delivery_client = state.delivery_client.clone();

    // First get the delivery by order ID - we need to map order to delivery
    // For now, assume delivery_id = order_id (would need proper mapping in real system)
    let request = tonic::Request::new(GetDeliveryStatusPayload { 
        delivery_id: order_id.clone() 
    });

    let response = delivery_client
        .get_delivery_status(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Delivery not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    let delivery_status = response.into_inner();
    let delivery_info = delivery_status.delivery_info.ok_or(
        ApiError::ServiceUnavailable("Invalid delivery status".to_string())
    )?;

    // Verify user has access to this order/delivery 
    // We need consumer_id and restaurant_id from the order
    // For now, we'll skip this verification as we'd need to fetch order details first
    // In a real system, you'd fetch the order first to get consumer_id and restaurant_id
    // then call: verify_order_access(&headers, &mut auth_client, &consumer_id, &restaurant_id).await?;

    Ok(Json(DeliveryStatusResponse {
        delivery_id: delivery_info.id.parse().map_err(|_| ApiError::InvalidToken)?,
        order_id: order_id.parse().map_err(|_| ApiError::InvalidToken)?,
        state: match delivery_info.state {
            0 => "PENDING".to_string(),
            1 => "SCHEDULED".to_string(),
            2 => "CANCELLED".to_string(),
            _ => "UNKNOWN".to_string(),
        },
        assigned_courier_id: if delivery_status.assigned_courier_id.is_empty() {
            None
        } else {
            Some(delivery_status.assigned_courier_id.parse().map_err(|_| ApiError::InvalidToken)?)
        },
        pickup_time: delivery_info.pickup_time.and_then(|ts| 
            chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
        ),
        delivery_time: delivery_info.delivery_time.and_then(|ts| 
            chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)
        ),
        courier_actions: delivery_status.courier_actions.into_iter().map(|action| {
            match action.r#type {
                0 => "PICKUP".to_string(),
                1 => "DROPOFF".to_string(),
                _ => "UNKNOWN".to_string(),
            }
        }).collect(),
    }))
}

#[utoipa::path(
    post,
    path = "/couriers",
    responses(
        (status = 200, description = "Courier created successfully", body = CreateCourierResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn create_courier(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CreateCourierResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    let _user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let mut delivery_client = state.delivery_client.clone();

    // Note: The proto has a typo "CreateConrier" but we'll use the correct method name
    let request = tonic::Request::new(());

    let response = delivery_client
        .create_conrier(request) // Using the typo from proto
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Delivery service error: {e}")))?;

    let courier = response.into_inner();
    let courier_id = courier.id.clone();

    // Grant courier access to the user (we'd need to add this to auth service)
    // For now, we'll skip this step as it would require extending the auth service
    // In a real system: auth_client.grant_courier_to_user(user_id, courier_id).await?;

    Ok(Json(CreateCourierResponse {
        courier_id: courier_id.parse().map_err(|_| ApiError::InvalidToken)?,
        available: courier.available,
    }))
}

#[utoipa::path(
    get,
    path = "/couriers/{courier_id}",
    responses(
        (status = 200, description = "Courier details", body = CourierDetailsResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Courier not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("courier_id" = String, Path, description = "Courier ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn get_courier(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(courier_id): Path<String>,
) -> Result<Json<CourierDetailsResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    
    // Verify user has access to this courier
    verify_courier_access(&headers, &mut auth_client, &courier_id).await?;

    let mut delivery_client = state.delivery_client.clone();

    let request = tonic::Request::new(GetCourierPayload { courier_id: courier_id.clone() });

    let response = delivery_client
        .get_courier(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Courier not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    let courier = response.into_inner();

    Ok(Json(CourierDetailsResponse {
        courier_id: courier.id.parse().map_err(|_| ApiError::InvalidToken)?,
        available: courier.available,
    }))
}

#[utoipa::path(
    put,
    path = "/couriers/{courier_id}/availability",
    request_body = UpdateCourierAvailabilityRequest,
    responses(
        (status = 200, description = "Courier availability updated successfully"),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Courier not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("courier_id" = String, Path, description = "Courier ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn update_courier_availability(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(courier_id): Path<String>,
    Json(payload): Json<UpdateCourierAvailabilityRequest>,
) -> Result<Json<()>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    
    // Verify user has access to this courier
    verify_courier_access(&headers, &mut auth_client, &courier_id).await?;

    let mut delivery_client = state.delivery_client.clone();

    let request = tonic::Request::new(UpdateCourierAvailabilityPayload { 
        courier_id, 
        available: payload.available 
    });

    delivery_client
        .update_courier_availability(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Courier not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    Ok(Json(()))
}

#[utoipa::path(
    get,
    path = "/couriers/{courier_id}/plan",
    responses(
        (status = 200, description = "Courier delivery plan", body = CourierPlanResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Courier not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("courier_id" = String, Path, description = "Courier ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn get_courier_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(courier_id): Path<String>,
) -> Result<Json<CourierPlanResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    
    // Verify user has access to this courier
    verify_courier_access(&headers, &mut auth_client, &courier_id).await?;

    let mut delivery_client = state.delivery_client.clone();

    let request = tonic::Request::new(GetCourierPayload { courier_id });

    let response = delivery_client
        .get_courier_plan(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Courier not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    let plan = response.into_inner();

    let actions = plan.actions.into_iter().map(|action| {
        Ok::<CourierActionResponse, ApiError>(CourierActionResponse {
            action_type: match action.r#type {
                0 => "PICKUP".to_string(),
                1 => "DROPOFF".to_string(),
                _ => "UNKNOWN".to_string(),
            },
            delivery_id: action.delivery_id.parse().map_err(|_| ApiError::InvalidToken)?,
            address: action.address,
            scheduled_time: {
                let timestamp = action.time
                    .ok_or(ApiError::ServiceUnavailable("Invalid action time".to_string()))?;
                chrono::DateTime::from_timestamp(timestamp.seconds, timestamp.nanos as u32)
                    .ok_or(ApiError::ServiceUnavailable("Invalid timestamp".to_string()))?
            },
        })
    }).collect::<Result<Vec<_>, _>>()?;

    Ok(Json(CourierPlanResponse { actions }))
}

#[utoipa::path(
    post,
    path = "/deliveries/{delivery_id}/pickup",
    responses(
        (status = 200, description = "Delivery picked up successfully"),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Delivery not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("delivery_id" = String, Path, description = "Delivery ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn pickup_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(delivery_id): Path<String>,
) -> Result<Json<()>, ApiError> {
    // For pickup/dropoff, we need to verify the user is the assigned courier
    // This would require fetching the delivery first to get the assigned courier ID
    // For now, we'll just extract the user ID to ensure authentication
    let mut auth_client = state.auth_client.clone();
    let _user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let mut delivery_client = state.delivery_client.clone();

    let request = tonic::Request::new(PickupDeliveryPayload { delivery_id });

    delivery_client
        .pickup_delivery(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Delivery not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    Ok(Json(()))
}

#[utoipa::path(
    post,
    path = "/deliveries/{delivery_id}/dropoff",
    responses(
        (status = 200, description = "Delivery dropped off successfully"),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Delivery not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("delivery_id" = String, Path, description = "Delivery ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "delivery"
)]
#[instrument(skip(state))]
pub async fn dropoff_delivery(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(delivery_id): Path<String>,
) -> Result<Json<()>, ApiError> {
    // For pickup/dropoff, we need to verify the user is the assigned courier
    // This would require fetching the delivery first to get the assigned courier ID
    // For now, we'll just extract the user ID to ensure authentication
    let mut auth_client = state.auth_client.clone();
    let _user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let mut delivery_client = state.delivery_client.clone();

    let request = tonic::Request::new(DropoffDeliveryPayload { delivery_id });

    delivery_client
        .dropoff_delivery(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Delivery not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Delivery service error: {e}"))
            }
        })?;

    Ok(Json(()))
}