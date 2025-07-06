use chrono::{DateTime, Utc};
use eventstore::{
    AppendToStreamOptions, EventData, ExpectedRevision, ReadStreamOptions, StreamMetadata,
    StreamPosition,
};
use serde::{Deserialize, Serialize};

use crate::establish_esdb_client;

#[derive(Deserialize, Serialize, Debug)]
pub struct CheckpointStored {
    subscription_id: String,
    position: u64,
    checkpointed_at: DateTime<Utc>,
}

pub struct CheckpointStore {
    client: eventstore::Client,
}

impl CheckpointStore {
    pub async fn store(
        &self,
        subscription_id: &str,
        position: u64,
    ) -> Result<(), eventstore::Error> {
        let stream_id = format!("checkpoint-{}", subscription_id);
        let event = CheckpointStored {
            subscription_id: subscription_id.to_string(),
            position: position,
            checkpointed_at: Utc::now(),
        };

        match self
            .client
            .append_to_stream(
                stream_id.clone(),
                &AppendToStreamOptions::default().expected_revision(ExpectedRevision::StreamExists),
                EventData::json("CheckpointStored", &event).unwrap(),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(eventstore::Error::WrongExpectedVersion {
                expected: _,
                current: _,
            }) => {
                // WrongExpectedVersionException means that stream did not exist

                // Set the checkpoint stream to have at most 1 event
                // using stream metadata $maxCount property
                let metadata = {
                    let mut metadata = StreamMetadata::default();
                    metadata.max_count = Some(1);
                    metadata
                };

                self.client
                    .set_stream_metadata(
                        stream_id.clone(),
                        &AppendToStreamOptions::default()
                            .expected_revision(ExpectedRevision::NoStream),
                        &metadata,
                    )
                    .await?;

                // append event again expecting stream to not exist
                self.client
                    .append_to_stream(
                        stream_id.clone(),
                        &AppendToStreamOptions::default()
                            .expected_revision(ExpectedRevision::NoStream),
                        EventData::json("CheckpointStored", &event).unwrap(),
                    )
                    .await
                    .map(|_| ())
            }
            Err(err) => Err(err),
        }
    }

    pub async fn load(&self, subscription_id: &str) -> Result<Option<u64>, eventstore::Error> {
        let stream_id = format!("checkpoint-{}", subscription_id);
        let mut stream = self
            .client
            .read_stream(
                stream_id,
                &ReadStreamOptions::default().position(StreamPosition::End),
            )
            .await?;

        match stream.next().await {
            Ok(Some(event)) => {
                let checkpoint = event
                    .get_original_event()
                    .as_json::<CheckpointStored>()
                    .expect("Invalid checkpoint");
                Ok(Some(checkpoint.position))
            }
            Ok(None) => Ok(None),
            Err(eventstore::Error::ResourceNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

impl Default for CheckpointStore {
    fn default() -> Self {
        Self {
            client: establish_esdb_client(),
        }
    }
}
