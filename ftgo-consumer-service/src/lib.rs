use std::env;

use diesel::{Connection, PgConnection};
use dotenvy::dotenv;

pub mod events;
pub mod models;
pub mod schema;

pub const EVENT_CHANNEL: &str = "consumer.event";
pub const COMMAND_CHANNEL: &str = "consumer.command";

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).unwrap()
}
