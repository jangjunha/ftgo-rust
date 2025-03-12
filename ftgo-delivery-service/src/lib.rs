use std::env;

use diesel::{Connection, PgConnection};
use dotenvy::dotenv;
use ftgo_proto::kitchen_service::kitchen_service_client::KitchenServiceClient;

pub mod events;
pub mod models;
pub mod schema;

pub const EVENT_CHANNEL: &str = "delivery.event";

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

pub async fn get_kitchen_client(
) -> Result<KitchenServiceClient<tonic::transport::Channel>, tonic::transport::Error> {
    dotenv().ok();

    let url = env::var("KITCHEN_SERVICE_URL").expect("KITCHEN_SERVICE_URL must be set");
    Ok(KitchenServiceClient::connect(url).await?)
}
