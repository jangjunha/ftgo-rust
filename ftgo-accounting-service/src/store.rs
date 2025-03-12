use std::env;

use dotenvy::dotenv;
use eventstore::{AppendToStreamOptions, EventData, ExpectedRevision, ReadStreamOptions};
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use prost::Message;
use uuid::Uuid;

use crate::models;

pub struct AccountStore {
    client: eventstore::Client,
}

impl AccountStore {
    pub async fn append(
        &self,
        id: &Uuid,
        events: &Vec<(Option<Uuid>, AccountingEvent)>,
        expected_revision: ExpectedRevision,
    ) -> Result<u64, eventstore::Error> {
        let stream_id = format!("Account-{}", id);
        self.client
            .append_to_stream(
                stream_id,
                &AppendToStreamOptions::default().expected_revision(expected_revision),
                events
                    .iter()
                    .map(|(event_id, accounting_event)| {
                        let event_type = match accounting_event.event.as_ref().unwrap() {
                            accounting_event::Event::AccountOpened(_) => "AccountOpened",
                            accounting_event::Event::AccountDeposited(_) => "AccountDeposited",
                            accounting_event::Event::AccountWithdrawn(_) => "AccountWithdrawn",
                        };
                        let event = EventData::binary(
                            event_type,
                            {
                                let mut buf = Vec::new();
                                accounting_event.encode(&mut buf).unwrap();
                                buf
                            }
                            .into(),
                        );
                        if let Some(event_id) = event_id {
                            event.id(*event_id)
                        } else {
                            event
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .await
            .map(|res| res.next_expected_version)
    }

    pub async fn get(&self, id: &Uuid) -> Result<models::Account, eventstore::Error> {
        let stream_id = format!("Account-{}", id);
        let mut stream = self
            .client
            .read_stream(stream_id, &ReadStreamOptions::default().forwards())
            .await?;

        let mut account = models::Account::new(id.clone());
        while let Some(event) = stream.next().await? {
            let event = AccountingEvent::decode(event.get_original_event().data.clone())
                .expect("Invalid accounting event");
            account = account.apply(event)
        }
        Ok(account)
    }
}

impl Default for AccountStore {
    fn default() -> Self {
        dotenv().ok();
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        Self {
            client: eventstore::Client::new(
                database_url
                    .parse()
                    .expect("Cannot parse esdb database url"),
            )
            .expect("Cannot construct esdb client"),
        }
    }
}
