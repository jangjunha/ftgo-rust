use axum::{Router, routing::post};
use dotenvy::dotenv;
use ftgo_proto::{
    auth_service::auth_service_client::AuthServiceClient,
    restaurant_service::restaurant_service_client::RestaurantServiceClient,
};
use tower_http::cors::CorsLayer;
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

mod error;
mod handlers;
mod models;

use handlers::{ApiDoc, AppState, create_restaurant, create_user, issue_token};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let auth_service_endpoint =
        std::env::var("AUTH_SERVICE_ENDPOINT").expect("AUTH_SERVICE_ENDPOINT required");
    let restaurant_service_endpoint =
        std::env::var("RESTAURANT_SERVICE_ENDPOINT").expect("RESTAURANT_SERVICE_ENDPOINT required");

    let auth_client = AuthServiceClient::connect(auth_service_endpoint).await?;
    let restaurant_client = RestaurantServiceClient::connect(restaurant_service_endpoint).await?;

    let state = AppState {
        auth_client,
        restaurant_client,
    };

    let app = Router::new()
        .route("/users", post(create_user))
        .route("/auth/token", post(issue_token))
        .route("/restaurants", post(create_restaurant))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8100").await?;
    info!("API Gateway listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}
