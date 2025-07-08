use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

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
    pub id: Uuid,
    /// Username of the user
    pub username: String,
    /// Timestamp when the user was created
    pub created_at: DateTime<Utc>,
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
    pub id: Uuid,
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateConsumerRequest {
    /// Name of the consumer
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateConsumerResponse {
    /// Unique identifier for the consumer
    pub id: Uuid,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Consumer {
    /// Unique identifier for the consumer
    pub id: Uuid,
    /// Name of the consumer
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct UserProfile {
    /// Unique identifier for the user
    pub id: Uuid,
    /// Username of the user
    pub username: String,
    /// Timestamp when the user was created
    pub created_at: DateTime<Utc>,
    /// List of restaurant IDs the user has access to
    pub granted_restaurants: Vec<Uuid>,
    /// List of consumer IDs the user has access to
    pub granted_consumers: Vec<Uuid>,
    /// List of courier IDs the user has access to
    pub granted_couriers: Vec<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Restaurant {
    /// Unique identifier for the restaurant
    pub id: Uuid,
    /// Name of the restaurant
    pub name: String,
    /// Address of the restaurant
    pub address: String,
    /// Menu items for the restaurant
    pub menu_items: Vec<MenuItemResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MenuItemResponse {
    /// Unique identifier for the menu item
    pub id: String,
    /// Name of the menu item
    pub name: String,
    /// Price of the menu item (as string)
    pub price: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListRestaurantsResponse {
    /// List of restaurants
    pub restaurants: Vec<Restaurant>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateOrderRequest {
    /// Restaurant ID to order from
    pub restaurant_id: Uuid,
    /// Consumer ID placing the order (must be owned by authenticated user)
    pub consumer_id: Uuid,
    /// List of menu items and quantities
    pub items: Vec<OrderItemRequest>,
    /// Delivery address
    pub delivery_address: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrderItemRequest {
    /// Menu item ID
    pub menu_item_id: String,
    /// Quantity of this item
    pub quantity: i32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateOrderResponse {
    /// Unique identifier for the order
    pub id: Uuid,
    /// Current state of the order
    pub state: String,
    /// Consumer ID who placed the order
    pub consumer_id: Uuid,
    /// Restaurant ID
    pub restaurant_id: Uuid,
    /// Order line items
    pub line_items: Vec<OrderLineItem>,
    /// Delivery information
    pub delivery_information: DeliveryInformation,
    /// Order minimum amount
    pub order_minimum: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrderLineItem {
    /// Quantity of this item
    pub quantity: i32,
    /// Menu item ID
    pub menu_item_id: String,
    /// Name of the menu item
    pub name: String,
    /// Price of the menu item
    pub price: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryInformation {
    /// Delivery time
    pub delivery_time: Option<DateTime<Utc>>,
    /// Delivery address
    pub delivery_address: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ApiErrorResponse {
    /// Error message
    pub error: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct KitchenTicket {
    /// Unique identifier for the ticket
    pub id: Uuid,
    /// Restaurant ID
    pub restaurant_id: Uuid,
    /// Order ID
    pub order_id: Option<Uuid>,
    /// Current state of the ticket
    pub state: String,
    /// Ticket line items
    pub line_items: Vec<TicketLineItem>,
    /// Ready by time
    pub ready_by: Option<DateTime<Utc>>,
    /// Accepted at time
    pub accepted_at: Option<DateTime<Utc>>,
    /// Preparing at time
    pub preparing_at: Option<DateTime<Utc>>,
    /// Ready for pickup at time
    pub ready_for_pickup_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TicketLineItem {
    /// Quantity of this item
    pub quantity: i32,
    /// Menu item ID
    pub menu_item_id: String,
    /// Name of the menu item
    pub name: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct KitchenTicketEdge {
    /// The ticket node
    pub node: KitchenTicket,
    /// Cursor for pagination
    pub cursor: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListTicketsResponse {
    /// List of kitchen ticket edges with cursor information
    pub edges: Vec<KitchenTicketEdge>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeliveryStatusResponse {
    /// Unique identifier for the delivery
    pub delivery_id: Uuid,
    /// Order ID this delivery is for
    pub order_id: Uuid,
    /// Current state of the delivery
    pub state: String,
    /// Assigned courier ID
    pub assigned_courier_id: Option<Uuid>,
    /// Pickup time
    pub pickup_time: Option<DateTime<Utc>>,
    /// Delivery time
    pub delivery_time: Option<DateTime<Utc>>,
    /// List of courier actions
    pub courier_actions: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountDetailsResponse {
    /// Account ID (equals Consumer ID)
    pub account_id: Uuid,
    /// Consumer ID who owns this account
    pub consumer_id: Uuid,
    /// Current account balance
    pub balance: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateCourierResponse {
    /// Unique identifier for the courier
    pub courier_id: Uuid,
    /// Availability status
    pub available: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CourierDetailsResponse {
    /// Unique identifier for the courier
    pub courier_id: Uuid,
    /// Availability status
    pub available: bool,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateCourierAvailabilityRequest {
    /// New availability status
    pub available: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CourierPlanResponse {
    /// List of planned courier actions
    pub actions: Vec<CourierActionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CourierActionResponse {
    /// Type of action (PICKUP or DROPOFF)
    pub action_type: String,
    /// Delivery ID
    pub delivery_id: Uuid,
    /// Address for the action
    pub address: String,
    /// Scheduled time for the action
    pub scheduled_time: DateTime<Utc>,
}
