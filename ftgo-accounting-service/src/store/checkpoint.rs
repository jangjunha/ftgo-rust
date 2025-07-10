use chrono::Utc;
use diesel::{insert_into, prelude::*, QueryResult};
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use futures::Stream;

use crate::{
    models::{Checkpoint, Event},
    schema,
};

pub struct CheckpointStore<'a> {
    subscription_id: String,
    conn: &'a mut AsyncPgConnection,
}

impl<'a> CheckpointStore<'a> {
    pub async fn store(
        &mut self,
        stream_name: &str,
        sequence: i64,
    ) -> Result<(), diesel::result::Error> {
        let checkpoint = Checkpoint {
            stream_name: stream_name.to_string(),
            subscription_id: self.subscription_id.to_string(),
            sequence,
            checkpointed_at: Utc::now(),
        };
        insert_into(schema::checkpoints::table)
            .values(&checkpoint)
            .on_conflict((
                schema::checkpoints::stream_name,
                schema::checkpoints::subscription_id,
            ))
            .do_update()
            .set(&checkpoint)
            .execute(self.conn)
            .await?;
        Ok(())
    }

    pub async fn retrieve_events_after_checkpoint(
        &mut self,
    ) -> Result<impl Stream<Item = QueryResult<Event>>, diesel::result::Error> {
        schema::events::table
            .inner_join(schema::event_stream::table)
            .left_join(
                schema::checkpoints::table.on(schema::checkpoints::subscription_id
                    .eq(&self.subscription_id)
                    .and(schema::checkpoints::stream_name.eq(schema::event_stream::name))),
            )
            .filter(schema::events::sequence.gt(schema::checkpoints::sequence))
            .or_filter(schema::checkpoints::sequence.is_null())
            .order_by((
                schema::events::stream_name.asc(),
                schema::events::sequence.asc(),
            ))
            .select(Event::as_select())
            .load_stream::<Event>(self.conn)
            .await
    }

    pub fn new(subscription_id: &str, conn: &'a mut AsyncPgConnection) -> Self {
        Self {
            subscription_id: subscription_id.to_string(),
            conn,
        }
    }
}
