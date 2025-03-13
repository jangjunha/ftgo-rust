use std::env;

use diesel::{Connection, PgConnection};
use dotenvy::dotenv;
use ftgo_proto::accounting_service::AccountingEvent;
use thiserror::Error;

pub mod account_details;
pub mod account_infos;
pub mod schema;

pub trait AccountingProjection {
    fn process(
        &mut self,
        event: &AccountingEvent,
        stream_position: i64,
        log_position: i64,
    ) -> Result<(), AccountingProjectionError>;
}

#[derive(Error, Debug)]
pub enum AccountingProjectionError {
    #[error("event {type_} key {key} is not valid")]
    InvalidEvent { type_: String, key: String },
    #[error("error while executing database query")]
    Connection(#[from] diesel::result::Error),
}

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}
