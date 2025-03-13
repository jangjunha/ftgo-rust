use std::env;

use diesel_async::{AsyncConnection, AsyncPgConnection};
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
    ) -> impl std::future::Future<Output = Result<(), AccountingProjectionError>> + Send;
}

#[derive(Error, Debug)]
pub enum AccountingProjectionError {
    #[error("event {type_} key {key} is not valid")]
    InvalidEvent { type_: String, key: String },
    #[error("error while executing database query")]
    Connection(#[from] diesel::result::Error),
}

pub async fn establish_connection() -> AsyncPgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    AsyncPgConnection::establish(&database_url)
        .await
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}
