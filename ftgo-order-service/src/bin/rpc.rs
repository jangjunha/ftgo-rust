use bigdecimal::BigDecimal;
use chrono::{Duration, Utc};
use diesel::{insert_into, prelude::*, update};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_order_service::events::OrderEventPublisher;
use ftgo_order_service::serializer::serialize_order_details;
use ftgo_proto::common::Money;
use ftgo_proto::order_service::{
    CreateOrderPayload, DeliveryInformation, GetOrderPayload, Order, OrderDetails, OrderLineItem,
    OrderState, PaymentInformation,
};
use prost::Message;
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

        conn.transaction(|conn| {
            insert_into(schema::orders::table)
                .values(&order)
                .execute(conn)?;
            insert_into(schema::order_line_items::table)
                .values(&line_items)
                .execute(conn)?;

            let mut publisher = OrderEventPublisher::new(conn);
            publisher.order_created(&order, &line_items, &restaurant)?;

            let saga_state = models::CreateOrderSagaState {
                order_id: order.id.clone(),
                order_details: {
                    let details = serialize_order_details(&order, &line_items, &restaurant);
                    let mut buf = Vec::new();
                    details.encode(&mut buf).unwrap();
                    buf
                },
                ticket_id: None,
            };
            // TODO: saga

            Ok(Response::new(serialize_order(order, line_items)))
        })
        .map_err(|_| Status::internal("Internal server error"))
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
