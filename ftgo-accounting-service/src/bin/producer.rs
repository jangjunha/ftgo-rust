use std::env;
use std::time::Duration;

use dotenvy::dotenv;
use eventstore::{Client, Position, StreamPosition, SubscribeToAllOptions, SubscriptionFilter};
use ftgo_proto::accounting_service::{accounting_event, AccountingEvent};
use ftgo_proto::common::CommandReply;
use kafka::client::RequiredAcks;
use kafka::producer::{Producer, Record};
use prost::Message;

use ftgo_accounting_service::establish_esdb_client;
use ftgo_accounting_service::store::checkpoint::CheckpointStore;

const ACCOUNTING_EVENT_TOPIC: &str = "accounting.event";
const CHECKPOINT_NAME: &str = "accounting-producer";

struct EventStoreProducer {
    client: Client,
    kafka: Producer,
    checkpoint_store: CheckpointStore,
}

impl EventStoreProducer {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

        let client = establish_esdb_client();
        let kafka = Producer::from_hosts(vec![kafka_url])
            .with_ack_timeout(Duration::from_secs(1))
            .with_required_acks(RequiredAcks::One)
            .create()?;

        let checkpoint_store = CheckpointStore::default();

        Ok(Self {
            client,
            kafka,
            checkpoint_store,
        })
    }

    async fn process_events(&mut self) -> Result<bool, Box<dyn std::error::Error>> {
        // Get the last checkpoint position
        let last_position = self.checkpoint_store.load(CHECKPOINT_NAME).await?;

        // Set up subscription options with filter for non-system events
        let mut options = SubscribeToAllOptions::default()
            .filter(SubscriptionFilter::on_event_type().regex("^[^\\$].*"));

        if let Some(position) = last_position {
            options = options.position(StreamPosition::Position(Position {
                commit: position,
                prepare: position,
            }));
        }

        // Subscribe to all events from EventStore
        let mut subscription = self.client.subscribe_to_all(&options).await;

        let mut processed_any = false;
        let mut latest_position = None;

        while let Ok(resolved_event) = subscription.next().await {
            let recorded_event = resolved_event.get_original_event();

            // Only process accounting events (skip system events and checkpoints)
            if recorded_event.stream_id.starts_with("Account-")
                && !recorded_event.event_type.starts_with("$")
                && recorded_event.event_type != "CheckpointStored"
            {
                if let Some(accounting_event) =
                    self.try_decode_accounting_event(&recorded_event.data)
                {
                    // Handle command reply events separately
                    if let Some(accounting_event::Event::CommandReplyRequested(reply_request)) = &accounting_event.event {
                        if let Some(reply) = &reply_request.reply {
                            self.publish_command_reply(reply, &reply_request.reply_channel)?;
                        }
                    } else {
                        // Publish regular accounting events
                        self.publish_accounting_event(&accounting_event, &recorded_event.stream_id)?;
                    }
                    processed_any = true;
                    latest_position = Some(recorded_event.position.commit);
                }
            }
        }

        // Update checkpoint if we processed any events
        if let Some(position) = latest_position {
            self.checkpoint_store
                .store(CHECKPOINT_NAME, position)
                .await?;
        }

        Ok(processed_any)
    }

    fn try_decode_accounting_event(&self, data: &[u8]) -> Option<AccountingEvent> {
        AccountingEvent::decode(data).ok()
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    println!("Starting Accounting EventStore Producer...");

    let mut producer = EventStoreProducer::new().await?;

    loop {
        match producer.process_events().await {
            Ok(true) => {
                // Processed some events, continue immediately
            }
            Ok(false) => {
                // No events to process, sleep for a bit
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(err) => {
                eprintln!("Error processing events: {:?}", err);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
