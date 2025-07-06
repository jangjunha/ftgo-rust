use axum::{Form, extract::State, response::Json, Router, routing::post};
use ftgo_proto::auth_service::{
    CreateUserPayload, CredentialType, IssueTokenPayload,
};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", post(create_user))
        .route("/auth/token", post(issue_token))
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
    request_body = IssueTokenRequest,
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