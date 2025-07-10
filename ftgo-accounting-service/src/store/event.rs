use diesel::{insert_into, prelude::*, update, QueryResult};
use diesel_async::{
    scoped_futures::ScopedFutureExt, AsyncConnection, AsyncPgConnection, RunQueryDsl,
};
use futures::Stream;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    models::{Event, EventStream, NewEvent},
    schema,
};

pub struct EventData {
    pub payload: Vec<u8>,
    pub metadata: Value,
}

#[derive(Debug)]
pub enum AppendCondition {
    NoStream,
    StreamExists,
    ExpectLastSequence(i64),
}

#[derive(Error, Debug)]
pub enum EventStoreError {
    #[error("append condition check failed")]
    AppendConditionFailed(AppendCondition),
    #[error("database error")]
    UnexpectedInternal(#[from] diesel::result::Error),
}

pub struct EventStore<'a> {
    pub stream_name: String,
    conn: &'a mut AsyncPgConnection,
}

impl<'a> EventStore<'a> {
    pub fn new(stream_name: &str, conn: &'a mut AsyncPgConnection) -> Self {
        Self {
            stream_name: stream_name.to_string(),
            conn,
        }
    }

    pub async fn append(
        &mut self,
        events: Vec<(Option<&Uuid>, EventData)>,
        condition: Option<AppendCondition>,
    ) -> Result<Vec<i64>, EventStoreError> {
        let stream_name = &self.stream_name;
        self.conn
            .transaction(|conn| {
                async move {
                    // Create EventStream if not exists
                    insert_into(schema::event_stream::table)
                        .values(EventStream {
                            name: stream_name.to_string(),
                            last_sequence: -1,
                        })
                        .on_conflict(schema::event_stream::name)
                        .do_nothing()
                        .execute(conn)
                        .await
                        .map_err(EventStoreError::UnexpectedInternal)?;

                    // Obtain a lock for the stream
                    let mut stream = schema::event_stream::table
                        .select(EventStream::as_select())
                        .find(stream_name)
                        .for_update()
                        .first::<EventStream>(conn)
                        .await
                        .map_err(EventStoreError::UnexpectedInternal)?;

                    // Check append condition
                    if let Some(condition) = condition {
                        if !(match condition {
                            AppendCondition::NoStream => stream.last_sequence < 0,
                            AppendCondition::StreamExists => stream.last_sequence >= 0,
                            AppendCondition::ExpectLastSequence(expected_sequence) => {
                                stream.last_sequence == expected_sequence
                            }
                        }) {
                            return Err(EventStoreError::AppendConditionFailed(condition));
                        }
                    }

                    let new_events = events
                        .into_iter()
                        .map(|(event_id, event_data)| {
                            stream.last_sequence += 1;
                            NewEvent {
                                stream_name: stream_name.to_string(),
                                id: event_id.cloned(),
                                sequence: stream.last_sequence,
                                payload: event_data.payload,
                                metadata: event_data.metadata,
                            }
                        })
                        .collect::<Vec<_>>();
                    insert_into(schema::events::table)
                        .values(&new_events)
                        .execute(conn)
                        .await
                        .map_err(EventStoreError::UnexpectedInternal)?;
                    update(schema::event_stream::table)
                        .set(&stream)
                        .execute(conn)
                        .await
                        .map_err(EventStoreError::UnexpectedInternal)?;

                    Ok(new_events
                        .into_iter()
                        .map(|event| event.sequence)
                        .collect::<Vec<_>>())
                }
                .scope_boxed()
            })
            .await
    }

    pub async fn read_stream(
        &mut self,
    ) -> Result<impl Stream<Item = QueryResult<Event>>, EventStoreError> {
        let stream = schema::events::table
            .select(Event::as_select())
            .filter(schema::events::stream_name.eq(self.stream_name.to_string()))
            .order_by((
                schema::events::stream_name.asc(),
                schema::events::sequence.asc(),
            ))
            .load_stream(self.conn)
            .await
            .map_err(|err| EventStoreError::UnexpectedInternal(err))?;
        Ok(stream)
    }
}
