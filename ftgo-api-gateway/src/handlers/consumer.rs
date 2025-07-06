use axum::{extract::{State, Path}, http::HeaderMap, response::Json, Router, routing::{post, get}};
use ftgo_proto::{
    auth_service::GrantConsumerToUserPayload,
    consumer_service::{CreateConsumerPayload, GetConsumerPayload},
};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, extract_user_id_from_token, verify_consumer_access};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/consumers", post(create_consumer))
        .route("/consumers/:id", get(get_consumer))
}

#[utoipa::path(
    post,
    path = "/consumers",
    request_body = CreateConsumerRequest,
    responses(
        (status = 200, description = "Consumer created successfully", body = CreateConsumerResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    security(
        ("bearer" = [])
    ),
    tag = "consumers"
)]
#[instrument(skip(state))]
pub async fn create_consumer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateConsumerRequest>,
) -> Result<Json<CreateConsumerResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    let user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let mut consumer_client = state.consumer_client.clone();

    let request = tonic::Request::new(CreateConsumerPayload {
        name: payload.name,
    });

    let response = consumer_client
        .create_consumer(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Consumer service error: {e}")))?;

    let consumer_id = response.into_inner().id;

    // Grant consumer access to the user
    let grant_request = tonic::Request::new(GrantConsumerToUserPayload {
        user_id,
        consumer_id: consumer_id.clone(),
    });

    auth_client
        .grant_consumer_to_user(grant_request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    Ok(Json(CreateConsumerResponse { 
        id: consumer_id.parse().map_err(|_| ApiError::InvalidToken)? 
    }))
}

#[utoipa::path(
    get,
    path = "/consumers/{id}",
    responses(
        (status = 200, description = "Consumer details", body = Consumer),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Consumer not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("id" = String, Path, description = "Consumer ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "consumers"
)]
#[instrument(skip(state))]
pub async fn get_consumer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(consumer_id): Path<String>,
) -> Result<Json<Consumer>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    
    // Verify user has access to this consumer
    verify_consumer_access(&headers, &mut auth_client, &consumer_id).await?;

    let mut consumer_client = state.consumer_client.clone();

    let request = tonic::Request::new(GetConsumerPayload { consumer_id });

    let response = consumer_client
        .get_consumer(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Consumer not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Consumer service error: {e}"))
            }
        })?;

    let response_data = response.into_inner();
    let consumer = response_data.consumer.ok_or(ApiError::ServiceUnavailable("Consumer not found".to_string()))?;

    Ok(Json(Consumer {
        id: consumer.id.parse().map_err(|_| ApiError::InvalidToken)?,
        name: consumer.name,
    }))
}