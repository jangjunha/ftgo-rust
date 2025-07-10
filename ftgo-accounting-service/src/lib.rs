use std::env;

use diesel_async::{AsyncConnection, AsyncPgConnection};
use dotenvy::dotenv;

pub mod aggregate;
pub mod models;
pub mod projection;
pub mod schema;
pub mod service;
pub mod store;

pub const EVENT_CHANNEL: &str = "accounting.event";
pub const COMMAND_CHANNEL: &str = "accounting.command";

pub async fn establish_connection() -> AsyncPgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    AsyncPgConnection::establish(&database_url).await.unwrap()
}
