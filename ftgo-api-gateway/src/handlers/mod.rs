pub mod accounting;
pub mod auth;
pub mod consumer;
pub mod delivery;
pub mod kitchen;
pub mod order;
pub mod restaurant;

// Re-export routers for easier importing
pub use accounting::router as accounting_router;
pub use auth::router as auth_router;
pub use consumer::router as consumer_router;
pub use delivery::router as delivery_router;
pub use kitchen::router as kitchen_router;
pub use order::router as order_router;
pub use restaurant::router as restaurant_router;

use axum::http::HeaderMap;
use ftgo_proto::auth_service::{
    GetTokenInfoPayload, GetUserPayload, auth_service_client::AuthServiceClient,
};
use tonic::transport::Channel;
use utoipa::OpenApi;

use crate::error::ApiError;

#[derive(Clone)]
pub struct AppState {
    pub auth_client: AuthServiceClient<Channel>,
    pub consumer_client:
        ftgo_proto::consumer_service::consumer_service_client::ConsumerServiceClient<Channel>,
    pub order_client: ftgo_proto::order_service::order_service_client::OrderServiceClient<Channel>,
    pub restaurant_client:
        ftgo_proto::restaurant_service::restaurant_service_client::RestaurantServiceClient<Channel>,
    pub kitchen_client:
        ftgo_proto::kitchen_service::kitchen_service_client::KitchenServiceClient<Channel>,
    pub delivery_client:
        ftgo_proto::delivery_service::delivery_service_client::DeliveryServiceClient<Channel>,
    pub accounting_client:
        ftgo_proto::accounting_service::accounting_service_client::AccountingServiceClient<Channel>,
}

// Shared utility functions
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

async fn verify_consumer_access(
    headers: &HeaderMap,
    auth_client: &mut AuthServiceClient<Channel>,
    consumer_id: &str,
) -> Result<(), ApiError> {
    let user_id = extract_user_id_from_token(headers, auth_client).await?;

    let request = tonic::Request::new(GetUserPayload { id: user_id });
    let response = auth_client
        .get_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();

    // Check if user has access to this consumer
    if user.granted_consumers.contains(&consumer_id.to_string()) {
        Ok(())
    } else {
        Err(ApiError::AuthenticationFailed)
    }
}

async fn verify_restaurant_access(
    headers: &HeaderMap,
    auth_client: &mut AuthServiceClient<Channel>,
    restaurant_id: &str,
) -> Result<(), ApiError> {
    let user_id = extract_user_id_from_token(headers, auth_client).await?;

    let request = tonic::Request::new(GetUserPayload { id: user_id });
    let response = auth_client
        .get_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();

    // Check if user has access to this restaurant
    if user
        .granted_restaurants
        .contains(&restaurant_id.to_string())
    {
        Ok(())
    } else {
        Err(ApiError::AuthenticationFailed)
    }
}

async fn verify_courier_access(
    headers: &HeaderMap,
    auth_client: &mut AuthServiceClient<Channel>,
    courier_id: &str,
) -> Result<(), ApiError> {
    let user_id = extract_user_id_from_token(headers, auth_client).await?;

    let request = tonic::Request::new(GetUserPayload { id: user_id });
    let response = auth_client
        .get_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();

    // Check if user has access to this courier
    if user.granted_couriers.contains(&courier_id.to_string()) {
        Ok(())
    } else {
        Err(ApiError::AuthenticationFailed)
    }
}

async fn verify_order_access(
    headers: &HeaderMap,
    auth_client: &mut AuthServiceClient<Channel>,
    consumer_id: &str,
    restaurant_id: &str,
) -> Result<(), ApiError> {
    let user_id = extract_user_id_from_token(headers, auth_client).await?;

    let request = tonic::Request::new(GetUserPayload { id: user_id });
    let response = auth_client
        .get_user(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Auth service error: {e}")))?;

    let user = response.into_inner();

    // Check if user has access to either the consumer (order owner) or restaurant (restaurant owner)
    let has_consumer_access = user.granted_consumers.contains(&consumer_id.to_string());
    let has_restaurant_access = user
        .granted_restaurants
        .contains(&restaurant_id.to_string());

    if has_consumer_access || has_restaurant_access {
        Ok(())
    } else {
        Err(ApiError::AuthenticationFailed)
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        auth::create_user,
        auth::issue_token,
        auth::get_user_profile,
        consumer::create_consumer,
        consumer::get_consumer,
        restaurant::create_restaurant,
        restaurant::list_restaurants,
        restaurant::get_restaurant,
        order::create_order,
        order::get_order,
        kitchen::list_tickets,
        kitchen::get_ticket,
        kitchen::accept_ticket,
        kitchen::preparing_ticket,
        kitchen::ready_for_pickup_ticket,
        delivery::get_delivery_status,
        delivery::create_courier,
        delivery::get_courier,
        delivery::update_courier_availability,
        delivery::get_courier_plan,
        delivery::pickup_delivery,
        delivery::dropoff_delivery,
        accounting::get_account,
    ),
    components(
        schemas(
            crate::models::CreateUserRequest,
            crate::models::CreateUserResponse,
            crate::models::UserProfile,
            crate::models::IssueTokenRequest,
            crate::models::IssueTokenResponse,
            crate::models::CreateConsumerRequest,
            crate::models::CreateConsumerResponse,
            crate::models::Consumer,
            crate::models::CreateRestaurantRequest,
            crate::models::CreateRestaurantResponse,
            crate::models::Restaurant,
            crate::models::MenuItemResponse,
            crate::models::ListRestaurantsResponse,
            crate::models::CreateOrderRequest,
            crate::models::CreateOrderResponse,
            crate::models::OrderItemRequest,
            crate::models::OrderLineItem,
            crate::models::DeliveryInformation,
            crate::models::MenuItemRequest,
            crate::models::KitchenTicket,
            crate::models::TicketLineItem,
            crate::models::ListTicketsResponse,
            crate::models::DeliveryStatusResponse,
            crate::models::AccountDetailsResponse,
            crate::models::CreateCourierResponse,
            crate::models::CourierDetailsResponse,
            crate::models::UpdateCourierAvailabilityRequest,
            crate::models::CourierPlanResponse,
            crate::models::CourierActionResponse,
            crate::models::ApiErrorResponse
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "users", description = "User management endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "consumers", description = "Consumer management endpoints"),
        (name = "restaurants", description = "Restaurant management endpoints"),
        (name = "orders", description = "Order management endpoints"),
        (name = "kitchen", description = "Kitchen management endpoints"),
        (name = "delivery", description = "Delivery tracking endpoints"),
        (name = "accounting", description = "Account balance endpoints")
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
            let password_flow = Password::new("/auth/token", Scopes::default());
            components.add_security_scheme(
                "bearer",
                SecurityScheme::OAuth2(OAuth2::new([Flow::Password(password_flow)])),
            );
        }
    }
}
