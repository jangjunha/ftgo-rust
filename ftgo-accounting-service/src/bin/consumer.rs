use std::{env, thread::sleep, time::Duration};

use dotenvy::dotenv;
use ftgo_accounting_service::{
    projection::establish_connection, service::AccountingService, store::AccountStore,
    COMMAND_CHANNEL,
};
use ftgo_proto::{
    accounting_service::AccountingCommand,
    consumer_service::{consumer_event, ConsumerEvent},
};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const CONSUMER_EVENT_CHANNEL: &'static str = "consumer.event";
const GROUP: &'static str = "accounting-service";

enum AcceptedMessage {
    AccountingCommand(AccountingCommand),
    ConsumerEvent(ConsumerEvent),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            COMMAND_CHANNEL => Some(AcceptedMessage::AccountingCommand(
                AccountingCommand::decode(value).expect("Cannot decode command"),
            )),
            CONSUMER_EVENT_CHANNEL => Some(AcceptedMessage::ConsumerEvent(
                ConsumerEvent::decode(value).expect("Cannot decode consumer event"),
            )),
            _ => None,
        }
    }

    async fn process(self, service: &AccountingService<'_>) -> Result<(), ()> {
        match self {
            AcceptedMessage::AccountingCommand(command_event) => {
                match command_event.command.unwrap() {
                    ftgo_proto::accounting_service::accounting_command::Command::Deposit(
                        command,
                    ) => {
                        let account_id = command.id.parse::<Uuid>().map_err(|_| ())?;
                        let _ = service
                            .deposit(
                                account_id,
                                command.amount.ok_or(())?.amount.parse().map_err(|_| ())?,
                                command.description,
                                None,
                            )
                            .await
                            .map_err(|_| ())?;
                        Ok(())
                    }
                    ftgo_proto::accounting_service::accounting_command::Command::Withdraw(
                        command,
                    ) => {
                        let account_id = command.id.parse::<Uuid>().map_err(|_| ())?;
                        let _ = service
                            .withdraw(
                                account_id,
                                command.amount.ok_or(())?.amount.parse().map_err(|_| ())?,
                                command.description,
                                None,
                            )
                            .await
                            .map_err(|_| ())?;
                        Ok(())
                    }
                }
            }

            AcceptedMessage::ConsumerEvent(consumer_event) => match consumer_event.event.unwrap() {
                consumer_event::Event::ConsumerCreated(event) => {
                    let account_id = event.id.parse::<Uuid>().map_err(|_| ())?;
                    service.create(Some(account_id)).await.map_err(|_| ())?;
                    Ok(())
                }
            },
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");
    let mut consumer = Consumer::from_hosts(vec![kafka_url])
        .with_topic(COMMAND_CHANNEL.to_string())
        .with_topic(CONSUMER_EVENT_CHANNEL.to_string())
        .with_group(GROUP.to_string())
        .with_fallback_offset(FetchOffset::Earliest)
        .with_offset_storage(Some(GroupOffsetStorage::Kafka))
        .create()
        .unwrap();

    let store = AccountStore::default();
    let conn = &mut establish_connection().await;
    let service = AccountingService::new(store, conn);

    loop {
        let mss = consumer.poll().expect("Cannont poll messages");
        if mss.is_empty() {
            sleep(Duration::from_secs(1));
            continue;
        }

        for ms in mss.iter() {
            for m in ms.messages() {
                match AcceptedMessage::from(ms.topic(), m.value) {
                    Some(message) => {
                        message.process(&service).await.expect(&format!(
                            "Failed to process message {} {}",
                            ms.topic(),
                            m.offset
                        ));
                    }
                    None => {}
                }
            }
            let _ = consumer.consume_messageset(ms);
        }
        consumer
            .commit_consumed()
            .expect("Error while commit consumed");
    }
}
