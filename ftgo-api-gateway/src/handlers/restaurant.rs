use axum::{extract::{State, Path}, http::HeaderMap, response::Json, Router, routing::{post, get}};
use ftgo_proto::{
    auth_service::GrantRestaurantToUserPayload,
    common::Money,
    restaurant_service::{CreateRestaurantPayload, MenuItem, GetRestaurantPayload},
};
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, extract_user_id_from_token};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/restaurants", post(create_restaurant).get(list_restaurants))
        .route("/restaurants/:id", get(get_restaurant))
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

    Ok(Json(CreateRestaurantResponse { 
        id: restaurant_id.parse().map_err(|_| ApiError::InvalidToken)? 
    }))
}

#[utoipa::path(
    get,
    path = "/restaurants",
    responses(
        (status = 200, description = "List of restaurants", body = ListRestaurantsResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    tag = "restaurants"
)]
#[instrument(skip(state))]
pub async fn list_restaurants(
    State(state): State<AppState>,
) -> Result<Json<ListRestaurantsResponse>, ApiError> {
    let mut restaurant_client = state.restaurant_client.clone();

    let request = tonic::Request::new(());

    let response = restaurant_client
        .list_restaurant(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Restaurant service error: {e}")))?;

    let restaurants_response = response.into_inner();
    let restaurants = restaurants_response
        .restaurants
        .into_iter()
        .map(|r| Ok::<Restaurant, ApiError>(Restaurant {
            id: r.id.parse().map_err(|_| ApiError::InvalidToken)?,
            name: r.name,
            address: r.address,
            menu_items: r
                .menu_items
                .into_iter()
                .map(|item| crate::models::MenuItemResponse {
                    id: item.id,
                    name: item.name,
                    price: item.price.map(|p| p.amount).unwrap_or_default(),
                })
                .collect(),
        }))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ListRestaurantsResponse { restaurants }))
}

#[utoipa::path(
    get,
    path = "/restaurants/{id}",
    responses(
        (status = 200, description = "Restaurant details", body = Restaurant),
        (status = 404, description = "Restaurant not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("id" = String, Path, description = "Restaurant ID")
    ),
    tag = "restaurants"
)]
#[instrument(skip(state))]
pub async fn get_restaurant(
    State(state): State<AppState>,
    Path(restaurant_id): Path<String>,
) -> Result<Json<Restaurant>, ApiError> {
    let mut restaurant_client = state.restaurant_client.clone();

    let request = tonic::Request::new(GetRestaurantPayload { restaurant_id });

    let response = restaurant_client
        .get_restaurant(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Restaurant not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Restaurant service error: {e}"))
            }
        })?;

    let response_data = response.into_inner();
    let restaurant = response_data.restaurant.ok_or(ApiError::ServiceUnavailable("Restaurant not found".to_string()))?;

    Ok(Json(Restaurant {
        id: restaurant.id.parse().map_err(|_| ApiError::InvalidToken)?,
        name: restaurant.name,
        address: restaurant.address,
        menu_items: restaurant
            .menu_items
            .into_iter()
            .map(|item| crate::models::MenuItemResponse {
                id: item.id,
                name: item.name,
                price: item.price.map(|p| p.amount).unwrap_or_default(),
            })
            .collect(),
    }))
}