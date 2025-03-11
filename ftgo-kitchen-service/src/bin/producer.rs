use std::env;
use std::{thread::sleep, time::Duration};

use diesel::result::Error as DieselError;
use diesel::{
    Connection, ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl, SelectableHelper,
};
use dotenvy::dotenv;
use ftgo_kitchen_service::models::Outbox;
use ftgo_kitchen_service::{establish_connection, schema};
use kafka::client::RequiredAcks;
use kafka::producer::{Producer, Record};

struct OutboxProcessor {
    kafka: Producer,
}

impl OutboxProcessor {
    fn process_next_outbox_row(
        &mut self,
        conn: &mut PgConnection,
    ) -> Result<bool, OutboxProcessingError> {
        use schema::outbox::dsl::*;

        conn.transaction::<_, OutboxProcessingError, _>(|conn| {
            let row = match outbox
                .select(Outbox::as_select())
                .order(schema::outbox::id.asc())
                .for_update()
                .skip_locked()
                .first::<Outbox>(conn)
            {
                Ok(row) => row,
                Err(DieselError::NotFound) => return Ok(false),
                Err(err) => return Err(OutboxProcessingError::Database(err)),
            };

            self.send_message(&row)
                .map_err(|err| OutboxProcessingError::Kafka(err))?;

            diesel::delete(outbox.filter(schema::outbox::id.eq(row.id)))
                .execute(conn)
                .map_err(|err| OutboxProcessingError::Database(err))?;

            Ok(true)
        })
    }

    fn send_message(&mut self, row: &Outbox) -> Result<(), kafka::Error> {
        self.kafka.send(&Record::from_key_value(
            &row.topic,
            row.key.clone(),
            row.value.clone(),
        ))
    }
}

fn main() {
    dotenv().ok();
    let kafka_url = env::var("KAFKA_URL").expect("KAFKA_URL must be set");

    let conn = &mut establish_connection();
    let producer = Producer::from_hosts(vec![kafka_url])
        .with_ack_timeout(Duration::from_secs(1))
        .with_required_acks(RequiredAcks::One)
        .create()
        .unwrap();

    let mut outbox_processor = OutboxProcessor { kafka: producer };

    loop {
        let result = outbox_processor.process_next_outbox_row(conn);

        match result {
            Ok(true) => {}
            Ok(false) => {
                sleep(Duration::from_secs(1));
            }
            Err(err) => {
                eprintln!("Error processing outbox row: {:?}", err);
                sleep(Duration::from_secs(1));
            }
        }
    }
}

#[derive(Debug)]
pub enum OutboxProcessingError {
    Database(DieselError),
    Kafka(kafka::Error),
}

impl From<DieselError> for OutboxProcessingError {
    fn from(err: DieselError) -> Self {
        OutboxProcessingError::Database(err)
    }
}
