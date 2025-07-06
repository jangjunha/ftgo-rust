use std::env;

use diesel::{Connection, PgConnection};
use dotenvy::dotenv;

pub mod command_handlers;
pub mod events;
pub mod models;
pub mod proxy;
pub mod saga;
pub mod schema;
pub mod serializer;

pub const EVENT_CHANNEL: &str = "order.event";
pub const COMMAND_CHANNEL: &str = "order.command";
pub const REPLY_CHANNEL: &str = "order.reply";

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url).unwrap()
}
