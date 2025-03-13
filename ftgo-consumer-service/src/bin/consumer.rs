use std::{collections::HashMap, env, thread::sleep, time::Duration};

use diesel::{dsl::insert_into, prelude::*, result::Error::NotFound, Connection, PgConnection};
use dotenvy::dotenv;
use ftgo_consumer_service::{
    establish_connection,
    models::{self, NewOutbox},
    schema, COMMAND_CHANNEL,
};
use ftgo_proto::{common::CommandReply, consumer_service::ConsumerCommand};
use kafka::{
    client::{FetchOffset, GroupOffsetStorage},
    consumer::Consumer,
};
use prost::Message;
use uuid::Uuid;

const GROUP: &'static str = "consumer-service";

enum AcceptedMessage {
    ConsumerCommand(ConsumerCommand),
}

impl AcceptedMessage {
    fn from(topic: &str, value: &[u8]) -> Option<Self> {
        match topic {
            COMMAND_CHANNEL => Some(AcceptedMessage::ConsumerCommand(
                ConsumerCommand::decode(value).expect("Cannot decode consumer command"),
            )),
            _ => None,
        }
    }

    fn reply(
        conn: &mut PgConnection,
        channel: &Option<String>,
        state: &HashMap<String, String>,
        succeed: bool,
        body: Option<Vec<u8>>,
    ) -> Result<(), diesel::result::Error> {
        if let Some(channel) = channel {
            use schema::outbox::dsl::*;

            let container = CommandReply {
                state: state.clone(),
                succeed: succeed,
                body: body,
            };
            let mut buf = Vec::new();
            container.encode(&mut buf).unwrap();

            insert_into(outbox)
                .values(NewOutbox {
                    topic: channel.to_string(),
                    key: "".to_string(),
                    value: buf,
                })
                .execute(conn)?;
        }

        Ok(())
    }

    fn process(self, conn: &mut PgConnection) -> Result<(), ()> {
        match self {
            AcceptedMessage::ConsumerCommand(consumer_command) => {
                match consumer_command.command.unwrap() {
                    ftgo_proto::consumer_service::consumer_command::Command::ValidateOrderByConsumer(
                        command,
                    ) => {
                        use schema::consumers::dsl::*;

                        let cid = command
                            .id
                            .parse::<Uuid>()
                            .expect("Cannot decode consumer_id");

                        let _ = conn
                            .transaction::<_, diesel::result::Error, _>(|conn| {
                                match consumers.select(self::models::Consumer::as_select())
                                .find(cid)
                                .first::<self::models::Consumer>(conn) {
                                    Ok(_consumer) => {
                                        Self::reply(
                                            conn,
                                            &consumer_command.reply_channel,
                                            &consumer_command.state,
                                            true,
                                            None,
                                        )?;
                                        Ok(())
                                    }
                                    Err(NotFound) => {
                                        Self::reply(
                                            conn,
                                            &consumer_command.reply_channel,
                                            &consumer_command.state,
                                            false,
                                            None,
                                        )?;
                                        Ok(())
                                    }
                                    Err(err) => Err(err)
                                }
                            })
                            .map_err(|_| ())?;
                        Ok(())
                    }
                }
            }
        }
    }
}

fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

    let mut conn = establish_connection();
    let mut consumer = Consumer::from_hosts(vec![kafka_url])
        .with_topic(COMMAND_CHANNEL.to_string())
        .with_group(GROUP.to_string())
        .with_fallback_offset(FetchOffset::Earliest)
        .with_offset_storage(Some(GroupOffsetStorage::Kafka))
        .create()
        .unwrap();

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
                        message.process(&mut conn).expect(&format!(
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
