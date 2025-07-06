use std::env;

use dotenvy::dotenv;

pub mod models;
pub mod projection;
pub mod service;
pub mod store;

pub const EVENT_CHANNEL: &str = "accounting.event";
pub const COMMAND_CHANNEL: &str = "accounting.command";

pub fn establish_esdb_client() -> eventstore::Client {
    dotenv().ok();
    let database_url = env::var("ESDB_URL").expect("ESDB_URL must be set");
    eventstore::Client::new(
        database_url
            .parse()
            .expect("Cannot parse esdb database url"),
    )
    .expect("Cannot construct esdb client")
}
