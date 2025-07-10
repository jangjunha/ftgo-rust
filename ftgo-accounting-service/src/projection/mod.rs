use ftgo_proto::accounting_service::AccountingEvent;
use thiserror::Error;

pub mod account_details;
pub mod account_infos;

pub trait AccountingProjection {
    fn process(
        &mut self,
        event: &AccountingEvent,
        sequence: i64,
    ) -> impl std::future::Future<Output = Result<(), AccountingProjectionError>> + Send;
}

#[derive(Error, Debug)]
pub enum AccountingProjectionError {
    #[error("event {type_} key {key} is not valid")]
    InvalidEvent { type_: String, key: String },
    #[error("error while executing database query")]
    Connection(#[from] diesel::result::Error),
}
