use diesel_async::{async_connection_wrapper::AsyncConnectionWrapper, AsyncPgConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use eventstore::{Position, StreamPosition, SubscribeToAllOptions, SubscriptionFilter};
use ftgo_accounting_service::{
    establish_esdb_client,
    projection::{
        account_details::AccountDetailsProjection, account_infos::AccountInfosProjection,
        establish_connection, AccountingProjection,
    },
    store::checkpoint::CheckpointStore,
};
use ftgo_proto::accounting_service::AccountingEvent;
use prost::Message;

const SUBSCRIPTION_ID: &'static str = "default";

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[tokio::main]
async fn main() {
    let conn = establish_connection().await;
    let mut async_wrapper: AsyncConnectionWrapper<AsyncPgConnection> =
        AsyncConnectionWrapper::from(conn);
    tokio::task::spawn_blocking(move || {
        async_wrapper.run_pending_migrations(MIGRATIONS).unwrap();
    })
    .await
    .expect("Error while run migration");

    let mut conn = establish_connection().await;
    let client = establish_esdb_client();
    let store = CheckpointStore::default();

    let checkpoint = store
        .load(SUBSCRIPTION_ID)
        .await
        .expect("Cannot retrieve checkpoint");

    let mut options = SubscribeToAllOptions::default()
        .filter(SubscriptionFilter::on_event_type().regex("^[^\\$].*"));
    if let Some(position) = checkpoint {
        options = options.position(StreamPosition::Position(Position {
            commit: position,
            prepare: position,
        }));
    };

    let mut subscription = client.subscribe_to_all(&options).await;
    while let Ok(resolved_event) = subscription.next().await {
        let recorded_event = resolved_event.get_original_event();
        if recorded_event.data.is_empty() {
            continue;
        }
        if recorded_event.event_type == "CheckpointStored" {
            continue;
        }

        let event = AccountingEvent::decode(recorded_event.data.clone())
            .expect("Failed to decode accounting event");

        AccountDetailsProjection::new(&mut conn)
            .process(
                &event,
                recorded_event.revision.try_into().unwrap(),
                recorded_event.position.commit.try_into().unwrap(),
            )
            .await
            .expect("Failed to process while AccountDetails projection");
        AccountInfosProjection::new(&mut conn)
            .process(
                &event,
                recorded_event.revision.try_into().unwrap(),
                recorded_event.position.commit.try_into().unwrap(),
            )
            .await
            .expect("Failed to process while AccountInfosProjection projection");

        store
            .store(SUBSCRIPTION_ID, recorded_event.position.commit)
            .await
            .unwrap();
    }
}
