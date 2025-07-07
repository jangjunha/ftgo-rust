use axum::{
    Form, Router,
    extract::State,
    http::HeaderMap,
    response::Json,
    routing::{get, post},
};
use ftgo_proto::auth_service::{CreateUserPayload, CredentialType, IssueTokenPayload};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, extract_user_id_from_token};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/auth/token", post(issue_token))
        .route("/me", get(get_user_profile))
}

#[utoipa::path(
    post,
    path = "/users",
    request_body = CreateUserRequest,
    responses(
        (status = 200, description = "User created successfully", body = CreateUserResponse),
        (status = 400, description = "Bad request", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    tag = "users"
)]
#[instrument(skip(state))]
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, ApiError> {
    let mut client = state.auth_client.clone();

    let request = tonic::Request::new(CreateUserPayload {
        username: payload.username,
        passphrase: payload.passphrase,
    });

    let response = client
        .create_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();
    let created_at = user
        .created_at
        .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
        .unwrap_or_else(chrono::Utc::now);

    Ok(Json(CreateUserResponse {
        id: user.id.parse().map_err(|_| ApiError::InvalidToken)?,
        username: user.username,
        created_at,
    }))
}

#[utoipa::path(
    post,
    path = "/auth/token",
    request_body(content = IssueTokenRequest, content_type = "application/x-www-form-urlencoded"),
    responses(
        (status = 200, description = "Token issued successfully", body = IssueTokenResponse),
        (status = 401, description = "Invalid credentials", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    tag = "auth"
)]
#[instrument(skip(state))]
pub async fn issue_token(
    State(state): State<AppState>,
    Form(payload): Form<IssueTokenRequest>,
) -> Result<Json<IssueTokenResponse>, ApiError> {
    let mut client = state.auth_client.clone();

    // Validate grant_type
    if payload.grant_type != "password" {
        return Err(ApiError::AuthenticationFailed);
    }

    let request = tonic::Request::new(IssueTokenPayload {
        credential_type: CredentialType::Passphrase as i32,
        sub: payload.password.clone(),
        username: Some(payload.username),
    });

    let response = client.issue_token(request).await.map_err(|e| {
        if e.code() == tonic::Code::InvalidArgument {
            ApiError::AuthenticationFailed
        } else {
            ApiError::ServiceUnavailable(format!("Auth service error: {e}"))
        }
    })?;

    let token_response = response.into_inner();

    Ok(Json(IssueTokenResponse {
        token_type: token_response.token_type,
        access_token: token_response.access_token,
        expires_in: token_response.expires_in,
    }))
}

#[utoipa::path(
    get,
    path = "/me",
    responses(
        (status = 200, description = "User profile with granted resources", body = UserProfile),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    security(
        ("bearer" = [])
    ),
    tag = "auth"
)]
#[instrument(skip(state))]
pub async fn get_user_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserProfile>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    let user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let request = tonic::Request::new(ftgo_proto::auth_service::GetUserPayload { id: user_id });
    let response = auth_client
        .get_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();
    let created_at = user
        .created_at
        .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32))
        .unwrap_or_else(chrono::Utc::now);

    // Parse granted resources, filtering out invalid UUIDs
    let granted_restaurants = user
        .granted_restaurants
        .into_iter()
        .filter_map(|id| id.parse().ok())
        .collect();

    let granted_consumers = user
        .granted_consumers
        .into_iter()
        .filter_map(|id| id.parse().ok())
        .collect();

    let granted_couriers = user
        .granted_couriers
        .into_iter()
        .filter_map(|id| id.parse().ok())
        .collect();

    Ok(Json(UserProfile {
        id: user.id.parse().map_err(|_| ApiError::InvalidToken)?,
        username: user.username,
        created_at,
        granted_restaurants,
        granted_consumers,
        granted_couriers,
    }))
}
