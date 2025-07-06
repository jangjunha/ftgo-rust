use axum::Router;
use dotenvy::dotenv;
use ftgo_proto::{
    auth_service::auth_service_client::AuthServiceClient,
    consumer_service::consumer_service_client::ConsumerServiceClient,
    order_service::order_service_client::OrderServiceClient,
    restaurant_service::restaurant_service_client::RestaurantServiceClient,
    kitchen_service::kitchen_service_client::KitchenServiceClient,
    delivery_service::delivery_service_client::DeliveryServiceClient,
    accounting_service::accounting_service_client::AccountingServiceClient,
};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod error;
mod handlers;
mod models;

use handlers::{ApiDoc, AppState, auth_router, consumer_router, restaurant_router, order_router, kitchen_router, delivery_router, accounting_router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let auth_service_endpoint =
        std::env::var("AUTH_SERVICE_ENDPOINT").expect("AUTH_SERVICE_ENDPOINT required");
    let consumer_service_endpoint =
        std::env::var("CONSUMER_SERVICE_ENDPOINT").expect("CONSUMER_SERVICE_ENDPOINT required");
    let order_service_endpoint =
        std::env::var("ORDER_SERVICE_ENDPOINT").expect("ORDER_SERVICE_ENDPOINT required");
    let restaurant_service_endpoint =
        std::env::var("RESTAURANT_SERVICE_ENDPOINT").expect("RESTAURANT_SERVICE_ENDPOINT required");
    let kitchen_service_endpoint =
        std::env::var("KITCHEN_SERVICE_ENDPOINT").expect("KITCHEN_SERVICE_ENDPOINT required");
    let delivery_service_endpoint =
        std::env::var("DELIVERY_SERVICE_ENDPOINT").expect("DELIVERY_SERVICE_ENDPOINT required");
    let accounting_service_endpoint =
        std::env::var("ACCOUNTING_SERVICE_ENDPOINT").expect("ACCOUNTING_SERVICE_ENDPOINT required");

    let auth_client = AuthServiceClient::connect(auth_service_endpoint).await?;
    let consumer_client = ConsumerServiceClient::connect(consumer_service_endpoint).await?;
    let order_client = OrderServiceClient::connect(order_service_endpoint).await?;
    let restaurant_client = RestaurantServiceClient::connect(restaurant_service_endpoint).await?;
    let kitchen_client = KitchenServiceClient::connect(kitchen_service_endpoint).await?;
    let delivery_client = DeliveryServiceClient::connect(delivery_service_endpoint).await?;
    let accounting_client = AccountingServiceClient::connect(accounting_service_endpoint).await?;

    let state = AppState {
        auth_client,
        consumer_client,
        order_client,
        restaurant_client,
        kitchen_client,
        delivery_client,
        accounting_client,
    };

    let app = Router::new()
        .merge(auth_router())
        .merge(consumer_router())
        .merge(restaurant_router())
        .merge(order_router())
        .merge(kitchen_router())
        .merge(delivery_router())
        .merge(accounting_router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8100").await?;
    info!("API Gateway listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
