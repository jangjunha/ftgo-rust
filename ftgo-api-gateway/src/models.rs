use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    /// Username for the new user
    pub username: String,
    /// Password for the new user
    pub passphrase: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateUserResponse {
    /// Unique identifier for the user
    pub id: String,
    /// Username of the user
    pub username: String,
    /// ISO 8601 timestamp when the user was created
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateRestaurantRequest {
    /// Name of the restaurant
    pub name: String,
    /// Address of the restaurant
    pub address: String,
    /// Menu items for the restaurant
    pub menu_items: Vec<MenuItemRequest>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MenuItemRequest {
    /// Unique identifier for the menu item
    pub id: String,
    /// Name of the menu item
    pub name: String,
    /// Price of the menu item (as string)
    pub price: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateRestaurantResponse {
    /// Unique identifier for the restaurant
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct IssueTokenRequest {
    /// Grant type (must be "password")
    pub grant_type: String,
    /// Username for authentication
    pub username: String,
    /// Password for authentication  
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IssueTokenResponse {
    /// Token type (e.g., "Bearer")
    pub token_type: String,
    /// Access token
    pub access_token: String,
    /// Token expiration time in seconds
    pub expires_in: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorResponse {
    /// Error message
    pub error: String,
}
