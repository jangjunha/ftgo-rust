use diesel::{insert_into, prelude::*, result::Error::NotFound};
use dotenvy::dotenv;
use ftgo_proto::consumer_service::{
    Consumer, CreateConsumerPayload, CreateConsumerResponse, GetConsumerPayload,
    GetConsumerResponse,
};
use std::env;
use tonic::{transport::Server, Request, Response, Status};
use uuid::Uuid;

use ftgo_proto::consumer_service::consumer_service_server::{
    ConsumerService, ConsumerServiceServer,
};

pub mod models;
pub mod schema;

pub fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgConnection::establish(&database_url)
        .unwrap_or_else(|_| panic!("Error connecting to {}", database_url))
}

#[derive(Default)]
pub struct ConsumerServiceImpl {}

#[tonic::async_trait]
impl ConsumerService for ConsumerServiceImpl {
    async fn create_consumer(
        &self,
        request: Request<CreateConsumerPayload>,
    ) -> Result<Response<CreateConsumerResponse>, Status> {
        use self::schema::consumers::dsl::*;

        let payload = request.into_inner();
        let consumer = models::Consumer {
            id: Uuid::new_v4(),
            name: payload.name,
        };

        let conn = &mut establish_connection();
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            insert_into(consumers).values(&consumer).execute(conn)?;
            Ok(())
        })
        .map_err(|_| Status::internal("Failed to create consumer"))?;

        Ok(Response::new(CreateConsumerResponse {
            id: consumer.id.to_string(),
        }))
    }

    async fn get_consumer(
        &self,
        request: Request<GetConsumerPayload>,
    ) -> Result<Response<GetConsumerResponse>, Status> {
        use self::schema::consumers::dsl::*;

        let payload = request.into_inner();
        let consumer_id = payload
            .consumer_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid consumer id"))?;

        let conn = &mut establish_connection();
        let result = match consumers
            .find(&consumer_id)
            .select(models::Consumer::as_select())
            .first(conn)
        {
            Ok(r) => Ok(r),
            Err(NotFound) => Err(Status::not_found("Consumer not found")),
            Err(_) => Err(Status::internal("Failed to get consumer")),
        }?;

        Ok(Response::new(GetConsumerResponse {
            consumer: Some(Consumer {
                id: result.id.to_string(),
                name: result.name,
            }),
        }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "[::1]:50052".parse().unwrap();
    let consumer_service = ConsumerServiceImpl::default();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<ConsumerServiceServer<ConsumerServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(ConsumerServiceServer::new(consumer_service))
        .serve(addr)
        .await?;

    Ok(())
}
