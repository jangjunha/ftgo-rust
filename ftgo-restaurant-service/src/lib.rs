use diesel::prelude::*;
use dotenvy::dotenv;
use std::env;

pub mod events;
pub mod models;
pub mod schema;

pub const EVENT_CHANNEL: &str = "restaurant.event";

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).unwrap()
}
