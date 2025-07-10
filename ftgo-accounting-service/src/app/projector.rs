use std::time::Duration;

use diesel_async::AsyncPgConnection;
use ftgo_accounting_service::{
    establish_connection,
    projection::{
        account_details::AccountDetailsProjection, account_infos::AccountInfosProjection,
        AccountingProjection,
    },
    store::checkpoint::CheckpointStore,
};
use ftgo_proto::accounting_service::AccountingEvent;
use futures::TryStreamExt;
use kafka::producer::AsBytes;
use prost::Message;

const SUBSCRIPTION_ID: &'static str = "projector";

struct Projector<'a> {
    store: CheckpointStore<'a>,
    conn: &'a mut AsyncPgConnection,
}

impl<'a> Projector<'a> {
    async fn process_once(&mut self) -> Result<usize, Box<dyn std::error::Error>> {
        let mut processed = 0;
        let mut stream = self.store.retrieve_events_after_checkpoint().await?;
        while let Some(event_row) = stream.try_next().await? {
            if event_row.stream_name.starts_with("Account-") {
                let event = AccountingEvent::decode(event_row.payload.as_bytes())
                    .expect("Failed to decode accounting event");

                AccountDetailsProjection::new(self.conn)
                    .process(&event, event_row.sequence)
                    .await
                    .expect("Failed to process while AccountDetails projection");
                AccountInfosProjection::new(self.conn)
                    .process(&event, event_row.sequence)
                    .await
                    .expect("Failed to process while AccountInfosProjection projection");

                self.store
                    .store(&event_row.stream_name, event_row.sequence)
                    .await?;
            }
            processed += 1;
        }
        Ok(processed)
    }

    pub async fn main(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            match self.process_once().await? {
                0 => tokio::time::sleep(Duration::from_secs(1)).await,
                _ => {}
            };
        }
    }
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = &mut establish_connection().await;
    let mut projection_conn = establish_connection().await;
    let store = CheckpointStore::new(SUBSCRIPTION_ID, conn);
    let mut projector = Projector {
        store,
        conn: &mut projection_conn,
    };
    projector.main().await
}
