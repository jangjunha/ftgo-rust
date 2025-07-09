use bigdecimal::BigDecimal;
use chrono::{Duration, Utc};
use diesel::{insert_into, prelude::*};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_order_service::events::OrderEventPublisher;
use ftgo_order_service::saga::create_order::{CreateOrderSaga, CreateOrderSagaState};
use ftgo_order_service::saga::SagaManager;
use ftgo_proto::common::Money;
use ftgo_proto::order_service::{
    CreateOrderPayload, DeliveryInformation, GetOrderPayload, ListOrderPayload, ListOrderResponse,
    Order, OrderEdge, OrderLineItem, OrderState, PaymentInformation,
};
use prost_types::Timestamp;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use ftgo_proto::order_service::order_service_server::{OrderService, OrderServiceServer};

use ftgo_order_service::{establish_connection, models, schema};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Default)]
pub struct OrderServiceImpl {}

#[tonic::async_trait]
impl OrderService for OrderServiceImpl {
    async fn get_order(
        &self,
        request: Request<GetOrderPayload>,
    ) -> Result<Response<Order>, Status> {
        let payload = request.into_inner();
        let oid: Uuid = payload
            .id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid order id"))?;

        let conn = &mut establish_connection();
        let order = match schema::orders::table
            .select(models::Order::as_select())
            .find(&oid)
            .get_result::<models::Order>(conn)
        {
            Ok(order) => order,
            Err(diesel::result::Error::NotFound) => {
                return Err(Status::not_found("order not found"))
            }
            Err(_) => return Err(Status::internal("Internal server error")),
        };
        let line_items = schema::order_line_items::table
            .select(models::OrderLineItem::as_select())
            .filter(schema::order_line_items::order_id.eq(&oid))
            .get_results(conn)
            .map_err(|_| Status::internal("Internal server error"))?;

        Ok(Response::new(serialize_order(order, line_items)))
    }

    async fn create_order(
        &self,
        request: Request<CreateOrderPayload>,
    ) -> Result<Response<Order>, Status> {
        println!("=== CREATE ORDER REQUEST RECEIVED ===");
        let payload = request.into_inner();
        let rid: Uuid = payload
            .restaurant_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid consumer id"))?;
        let cid: Uuid = payload
            .consumer_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid consumer id"))?;

        let conn = &mut establish_connection();
        let restaurant = schema::restaurants::table
            .select(models::Restaurant::as_select())
            .find(&rid)
            .get_result::<models::Restaurant>(conn)
            .map_err(|_| Status::invalid_argument("Restaurant not exists"))?;
        let restaurant_menu_items = schema::restaurant_menu_items::table
            .select(models::RestaurantMenuItem::as_select())
            .filter(schema::restaurant_menu_items::restaurant_id.eq(&rid))
            .get_results(conn)
            .map_err(|_| Status::internal("Internal server error"))?;

        let order = models::Order {
            id: Uuid::new_v4(),
            version: 1,
            state: models::OrderState::ApprovalPending,
            consumer_id: cid,
            restaurant_id: restaurant.id.clone(),
            delivery_time: Utc::now() + Duration::minutes(60),
            delivery_address: payload.delivery_address.clone(),
            payment_token: None,
            created_at: Utc::now(),
        };
        let line_items = payload
            .items
            .into_iter()
            .map(|i| -> Result<_, Status> {
                let menu_item = restaurant_menu_items
                    .iter()
                    .find(|m| m.id == i.menu_item_id)
                    .ok_or(Status::invalid_argument(format!(
                        "Menu item {} not exists",
                        i.menu_item_id
                    )))?;
                Ok(models::OrderLineItem {
                    id: Uuid::new_v4(),
                    order_id: order.id.clone(),
                    quantity: i.quantity,
                    menu_item_id: menu_item.id.clone(),
                    name: menu_item.name.clone(),
                    price: menu_item.price.clone(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        println!("Starting transaction");
        conn.transaction(|conn| {
            println!("Inserting order into database");
            insert_into(schema::orders::table)
                .values(&order)
                .execute(conn)?;
            insert_into(schema::order_line_items::table)
                .values(&line_items)
                .execute(conn)?;

            println!("Publishing order created event");
            let mut publisher = OrderEventPublisher::new(conn);
            publisher.order_created(&order, &line_items, &restaurant)?;

            println!("Starting create order saga");
            let saga_data = CreateOrderSagaState::new(&order.id, &line_items, &rid, &cid);
            let saga = CreateOrderSaga::new();
            let mut saga_manager = SagaManager::new(saga, conn);
            saga_manager.create(saga_data)?;

            println!("Transaction completed successfully");
            Ok(Response::new(serialize_order(order, line_items)))
        })
        .map_err(|e: diesel::result::Error| {
            println!("Transaction failed with error: {:?}", e);
            Status::internal("Internal server error")
        })
    }

    async fn list_order(
        &self,
        request: Request<ListOrderPayload>,
    ) -> Result<Response<ListOrderResponse>, Status> {
        let payload = request.into_inner();
        let conn = &mut establish_connection();

        let mut query = schema::orders::table
            .select(models::Order::as_select())
            .into_boxed();

        if let Some(consumer_id) = payload.consumer_id {
            let cid: Uuid = consumer_id
                .parse()
                .map_err(|_| Status::invalid_argument("Invalid consumer id"))?;
            query = query.filter(schema::orders::consumer_id.eq(cid));
        }

        if let Some(restaurant_id) = payload.restaurant_id {
            let rid: Uuid = restaurant_id
                .parse()
                .map_err(|_| Status::invalid_argument("Invalid restaurant id"))?;
            query = query.filter(schema::orders::restaurant_id.eq(rid));
        }

        if let Some(state) = payload.state {
            let order_state = models::OrderState::from(OrderState::try_from(state).unwrap());
            query = query.filter(schema::orders::state.eq(order_state));
        }

        let limit = payload.first.unwrap_or(10).min(100) as i64;

        if let Some(after) = payload.after {
            // Parse cursor as "timestamp:order_id"
            let parts: Vec<&str> = after.split(':').collect();
            if parts.len() != 2 {
                return Err(Status::invalid_argument("Invalid cursor format"));
            }

            let after_timestamp = parts[0]
                .parse::<i64>()
                .map_err(|_| Status::invalid_argument("Invalid cursor timestamp"))?;
            let after_order_id = parts[1]
                .parse::<uuid::Uuid>()
                .map_err(|_| Status::invalid_argument("Invalid cursor order_id"))?;

            let after_datetime = chrono::DateTime::from_timestamp(after_timestamp, 0)
                .ok_or_else(|| Status::invalid_argument("Invalid cursor timestamp"))?;

            // Use compound cursor for pagination with DESC order: (created_at < after_datetime) OR (created_at = after_datetime AND id < after_order_id)
            query = query.filter(
                schema::orders::created_at
                    .lt(after_datetime)
                    .or(schema::orders::created_at
                        .eq(after_datetime)
                        .and(schema::orders::id.gt(after_order_id))),
            );
        }

        let orders = query
            .order((schema::orders::created_at.desc(), schema::orders::id.asc()))
            .limit(limit)
            .get_results::<models::Order>(conn)
            .map_err(|_| Status::internal("Internal server error"))?;

        let edges = orders
            .into_iter()
            .map(|order| {
                let line_items = schema::order_line_items::table
                    .select(models::OrderLineItem::as_select())
                    .filter(schema::order_line_items::order_id.eq(&order.id))
                    .get_results(conn)
                    .map_err(|_| Status::internal("Internal server error"))?;

                Ok(OrderEdge {
                    node: Some(serialize_order(order.clone(), line_items)),
                    cursor: format!("{}:{}", order.created_at.timestamp(), order.id),
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;

        Ok(Response::new(ListOrderResponse { edges }))
    }
}

fn serialize_order(order: models::Order, line_items: Vec<models::OrderLineItem>) -> Order {
    let order_minimum: BigDecimal = line_items.iter().map(|i| i.price.clone()).sum();
    Order {
        id: order.id.to_string(),
        state: OrderState::from(order.state).into(),
        consumer_id: order.consumer_id.to_string(),
        restaurant_id: order.restaurant_id.to_string(),
        line_items: line_items
            .into_iter()
            .map(|i| OrderLineItem {
                quantity: i.quantity,
                menu_item_id: i.menu_item_id.to_string(),
                name: i.name.to_string(),
                price: Some(Money {
                    amount: i.price.to_string(),
                }),
            })
            .collect(),
        delivery_information: Some(DeliveryInformation {
            delivery_time: Some(Timestamp {
                seconds: order.delivery_time.timestamp(),
                nanos: order.delivery_time.timestamp_subsec_nanos() as i32,
            }),
            delivery_address: order.delivery_address.to_string(),
        }),
        payment_information: order.payment_token.map(|t| PaymentInformation {
            payment_token: t.to_string(),
        }),
        order_minimum: Some(Money {
            amount: order_minimum.to_string(),
        }),
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection();
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    let addr = "0.0.0.0:8103".parse().unwrap();
    let order_service = OrderServiceImpl::default();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<OrderServiceServer<OrderServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(OrderServiceServer::new(order_service))
        .serve(addr)
        .await?;

    Ok(())
}
