use std::env;
use std::time::Duration;

use diesel_async::AsyncPgConnection;
use dotenvy::dotenv;
use ftgo_accounting_service::establish_connection;
use ftgo_accounting_service::store::checkpoint::CheckpointStore;
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use ftgo_proto::common::CommandReply;
use futures::TryStreamExt;
use kafka::client::RequiredAcks;
use kafka::producer::{AsBytes, Producer, Record};
use prost::Message;

const ACCOUNTING_EVENT_TOPIC: &str = "accounting.event";
const CHECKPOINT_NAME: &str = "accounting-producer";

struct EventStoreProducer<'a> {
    store: CheckpointStore<'a>,
    kafka: Producer,
}

impl<'a> EventStoreProducer<'a> {
    async fn new(conn: &'a mut AsyncPgConnection) -> Result<Self, Box<dyn std::error::Error>> {
        let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");
        let kafka = Producer::from_hosts(vec![kafka_url])
            .with_ack_timeout(Duration::from_secs(1))
            .with_required_acks(RequiredAcks::One)
            .create()?;

        let store = CheckpointStore::new(CHECKPOINT_NAME, conn);

        Ok(Self { store, kafka })
    }

    async fn process_events(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut processed_any = false;
        let mut stream = self.store.retrieve_events_after_checkpoint().await?;
        while let Some(event) = stream.try_next().await? {
            if event.stream_name.starts_with("Account-") {
                let accounting_event = AccountingEvent::decode(event.payload.as_bytes()).unwrap();

                // Handle command reply events separately
                if let Some(accounting_event::Event::CommandReplyRequested(reply_request)) =
                    &accounting_event.event
                {
                    if let Some(reply) = &reply_request.reply {
                        self.publish_command_reply(reply, &reply_request.reply_channel)?;
                    }
                } else {
                    // Publish regular accounting events
                    self.publish_accounting_event(&accounting_event, &event.stream_name)?;
                }
            }

            self.store.store(&event.stream_name, event.sequence).await?;

            processed_any = true;
        }
        Ok(processed_any)
    }

    fn publish_accounting_event(
        &mut self,
        event: &AccountingEvent,
        stream_id: &str,
    ) -> Result<(), kafka::Error> {
        // Log the event type for debugging
        let event_type = match &event.event {
            Some(accounting_event::Event::AccountOpened(_)) => "AccountOpened",
            Some(accounting_event::Event::AccountDeposited(_)) => "AccountDeposited",
            Some(accounting_event::Event::AccountWithdrawn(_)) => "AccountWithdrawn",
            // Skip internal event
            Some(accounting_event::Event::CommandReplyRequested(_)) => {
                return Ok(());
            }
            None => "Unknown",
        };

        // Extract account ID from stream name (Account-{uuid})
        let account_id = stream_id.strip_prefix("Account-").unwrap_or(stream_id);

        // Serialize the event
        let mut buf = Vec::new();
        event.encode(&mut buf).unwrap();

        // Create Kafka record
        let record = Record::from_key_value(ACCOUNTING_EVENT_TOPIC, account_id.to_string(), buf);

        // Send to Kafka
        self.kafka.send(&record)?;

        println!("Published {} event for account {}", event_type, account_id);

        Ok(())
    }

    fn publish_command_reply(
        &mut self,
        reply: &CommandReply,
        reply_channel: &str,
    ) -> Result<(), kafka::Error> {
        // Serialize the command reply
        let mut buf = Vec::new();
        reply.encode(&mut buf).unwrap();

        // Create Kafka record - use a dummy key since replies don't need specific partitioning
        let record = Record::from_key_value(reply_channel, "reply".to_string(), buf);

        // Send to the specified reply channel
        self.kafka.send(&record)?;

        println!("Published command reply to channel {}", reply_channel);

        Ok(())
    }
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();
    let conn = &mut establish_connection().await;

    println!("Starting Accounting EventStore Producer...");

    let mut producer = EventStoreProducer::new(conn)
        .await
        .expect("Failed to initiate");

    loop {
        match producer.process_events().await.unwrap() {
            true => {
                // Processed some events, continue immediately
            }
            false => {
                // No events to process, sleep for a bit
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}
