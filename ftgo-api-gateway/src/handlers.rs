use axum::{Form, extract::State, http::HeaderMap, response::Json};
use ftgo_proto::{
    auth_service::{
        CreateUserPayload, CredentialType, GetTokenInfoPayload, GrantRestaurantToUserPayload,
        IssueTokenPayload, auth_service_client::AuthServiceClient,
    },
    common::Money,
    restaurant_service::{
        CreateRestaurantPayload, MenuItem, restaurant_service_client::RestaurantServiceClient,
    },
};
use tonic::transport::Channel;
use tracing::instrument;
use utoipa::OpenApi;

use crate::error::ApiError;
use crate::models::*;

#[derive(Clone)]
pub struct AppState {
    pub auth_client: AuthServiceClient<Channel>,
    pub restaurant_client: RestaurantServiceClient<Channel>,
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
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();

    Ok(Json(CreateUserResponse {
        id: user.id,
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

async fn extract_user_id_from_token(
    headers: &HeaderMap,
    auth_client: &mut AuthServiceClient<Channel>,
) -> Result<String, ApiError> {
    let auth_header = headers
        .get("authorization")
        .ok_or(ApiError::AuthenticationFailed)?
        .to_str()
        .map_err(|_| ApiError::InvalidToken)?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or(ApiError::InvalidToken)?;

    let request = tonic::Request::new(GetTokenInfoPayload {
        token: token.to_string(),
    });

    let response = auth_client
        .get_token_info(request)
        .await
        .map_err(|_| ApiError::InvalidToken)?;

    Ok(response.into_inner().user_id)
}

#[utoipa::path(
    post,
    path = "/restaurants",
    request_body = CreateRestaurantRequest,
    responses(
        (status = 200, description = "Restaurant created successfully", body = CreateRestaurantResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    security(
        ("bearer" = []),
    ),
    tag = "restaurants"
)]
#[instrument(skip(state))]
pub async fn create_restaurant(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateRestaurantRequest>,
) -> Result<Json<CreateRestaurantResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    let user_id = extract_user_id_from_token(&headers, &mut auth_client).await?;

    let mut restaurant_client = state.restaurant_client.clone();

    let menu_items = payload
        .menu_items
        .into_iter()
        .map(|item| MenuItem {
            id: item.id,
            name: item.name,
            price: Some(Money { amount: item.price }),
        })
        .collect();

    let request = tonic::Request::new(CreateRestaurantPayload {
        name: payload.name,
        address: payload.address,
        menu_items,
    });

    let response = restaurant_client
        .create_restaurant(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Restaurant service error: {e}")))?;

    let restaurant_id = response.into_inner().id;

    // Grant restaurant access to the user
    let grant_request = tonic::Request::new(GrantRestaurantToUserPayload {
        user_id,
        restaurant_id: restaurant_id.clone(),
    });

    auth_client
        .grant_restaurant_to_user(grant_request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    Ok(Json(CreateRestaurantResponse { id: restaurant_id }))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        create_user,
        issue_token,
        create_restaurant,
    ),
    components(
        schemas(CreateUserRequest, CreateUserResponse, IssueTokenRequest, IssueTokenResponse, CreateRestaurantRequest, CreateRestaurantResponse, MenuItemRequest, ApiErrorResponse)
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "users", description = "User management endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "restaurants", description = "Restaurant management endpoints")
    ),
    info(
        title = "FTGO API Gateway",
        description = "API Gateway for FTGO microservices",
        version = "1.0.0"
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            use utoipa::openapi::security::*;
            components.add_security_scheme(
                "bearer",
                SecurityScheme::OAuth2(OAuth2::new([Flow::Password(Password::new(
                    "/auth/token",
                    Scopes::default(),
                ))])),
            );
        }
    }
}
