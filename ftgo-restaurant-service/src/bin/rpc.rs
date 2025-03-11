use bigdecimal::BigDecimal;
use diesel::{insert_into, prelude::*};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_proto::common::Money;
use ftgo_restaurant_service::events::RestaurantEventPublisher;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use ftgo_proto::restaurant_service::restaurant_service_server::{
    RestaurantService, RestaurantServiceServer,
};
use ftgo_proto::restaurant_service::{
    CreateRestaurantPayload, CreateRestaurantResponse, GetRestaurantPayload, GetRestaurantResponse,
    ListRestaurantsResponse, MenuItem, Restaurant,
};

use ftgo_restaurant_service::{establish_connection, models, schema};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Default)]
pub struct RestaurantServiceImpl {}

#[tonic::async_trait]
impl RestaurantService for RestaurantServiceImpl {
    async fn create_restaurant(
        &self,
        request: Request<CreateRestaurantPayload>,
    ) -> Result<Response<CreateRestaurantResponse>, Status> {
        use crate::schema::restaurant_menu_items::dsl::*;
        use crate::schema::restaurants::dsl::*;

        let payload = request.into_inner();
        let restaurant = models::Restaurant {
            id: Uuid::new_v4(),
            name: payload.name,
            address: payload.address,
        };
        let menu_items = payload
            .menu_items
            .into_iter()
            .map(|i| {
                Ok(models::RestaurantMenuItem {
                    restaurant_id: restaurant.id,
                    id: i.id,
                    name: i.name,
                    price: i
                        .price
                        .ok_or(Status::invalid_argument("Price required"))?
                        .amount
                        .parse::<BigDecimal>()
                        .map_err(|_| Status::invalid_argument("Invalid price"))?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;

        let conn = &mut establish_connection();
        conn.transaction::<_, diesel::result::Error, _>(|conn| {
            insert_into(restaurants).values(&restaurant).execute(conn)?;
            insert_into(restaurant_menu_items)
                .values(&menu_items)
                .execute(conn)?;

            let mut publisher = RestaurantEventPublisher::new(conn);
            publisher.restaurant_created(&restaurant, &menu_items);

            Ok(())
        })
        .map_err(|_| Status::internal("Failed to create restaurant"))?;

        Ok(Response::new(CreateRestaurantResponse {
            id: restaurant.id.to_string(),
        }))
    }

    async fn get_restaurant(
        &self,
        request: Request<GetRestaurantPayload>,
    ) -> Result<Response<GetRestaurantResponse>, Status> {
        use schema::restaurants::dsl::*;

        let payload = request.into_inner();
        let restaurant_id = payload
            .restaurant_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument("Invalid restaurant id"))?;

        let conn = &mut establish_connection();
        let result = restaurants
            .find(&restaurant_id)
            .select(models::Restaurant::as_select())
            .first(conn)
            .expect("Error loading restaurants");

        let menu_items = models::RestaurantMenuItem::belonging_to(&result)
            .select(models::RestaurantMenuItem::as_select())
            .load(conn)
            .expect("Error loading restaurant_menu_items");

        Ok(Response::new(GetRestaurantResponse {
            restaurant: Some(Restaurant {
                id: result.id.to_string(),
                name: result.name,
                address: result.address,
                menu_items: menu_items
                    .into_iter()
                    .map(|i| MenuItem {
                        id: i.id,
                        name: i.name,
                        price: Some(Money {
                            amount: i.price.to_string(),
                        }),
                    })
                    .collect(),
            }),
        }))
    }

    async fn list_restaurant(
        &self,
        _: Request<()>,
    ) -> Result<Response<ListRestaurantsResponse>, Status> {
        use schema::restaurants::dsl::*;

        let conn = &mut establish_connection();
        let results = restaurants
            .select(models::Restaurant::as_select())
            .load(conn)
            .expect("Error loading restaurants");

        let menu_items = models::RestaurantMenuItem::belonging_to(&results)
            .select(models::RestaurantMenuItem::as_select())
            .load(conn)
            .expect("Error loading restaurant_menu_items")
            .grouped_by(&results);

        Ok(Response::new(ListRestaurantsResponse {
            restaurants: results
                .into_iter()
                .zip(menu_items)
                .map(|(r, menu_items)| Restaurant {
                    id: r.id.to_string(),
                    name: r.name,
                    address: r.address,
                    menu_items: menu_items
                        .into_iter()
                        .map(|i| MenuItem {
                            id: i.id,
                            name: i.name,
                            price: Some(Money {
                                amount: i.price.to_string(),
                            }),
                        })
                        .collect(),
                })
                .collect(),
        }))
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection();
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    let addr = "0.0.0.0:8101".parse().unwrap();
    let restaurant_service = RestaurantServiceImpl::default();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<RestaurantServiceServer<RestaurantServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(RestaurantServiceServer::new(restaurant_service))
        .serve(addr)
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use diesel::RunQueryDsl;
    use ftgo_proto::common::Money;
    use ftgo_proto::restaurant_service::{CreateRestaurantPayload, GetRestaurantPayload, MenuItem};
    use tonic::Request;
    use uuid::Uuid;

    // 테스트 시작 전에 데이터베이스를 초기화하는 함수
    fn setup_database() {
        let conn = &mut establish_connection();
        diesel::delete(schema::restaurant_menu_items::table)
            .execute(conn)
            .unwrap();
        diesel::delete(schema::restaurants::table)
            .execute(conn)
            .unwrap();
    }

    #[tokio::test]
    async fn test_create_restaurant() {
        setup_database();

        let service = RestaurantServiceImpl::default();
        let payload = CreateRestaurantPayload {
            name: "Test Restaurant".to_string(),
            address: "Test Address".to_string(),
            menu_items: vec![
                MenuItem {
                    id: "item1".to_string(),
                    name: "Item 1".to_string(),
                    price: Some(Money {
                        amount: "10.00".to_string(),
                    }),
                },
                MenuItem {
                    id: "item2".to_string(),
                    name: "Item 2".to_string(),
                    price: Some(Money {
                        amount: "20.00".to_string(),
                    }),
                },
            ],
        };
        let request = Request::new(payload);
        let response = service.create_restaurant(request).await.unwrap();
        let restaurant_id = response.into_inner().id;

        assert!(!restaurant_id.is_empty());

        // 데이터베이스에서 레스토랑과 메뉴 항목이 정상적으로 생성되었는지 확인
        let conn = &mut establish_connection();
        let created_restaurant = schema::restaurants::table
            .find(restaurant_id.parse::<Uuid>().unwrap())
            .first::<models::Restaurant>(conn)
            .unwrap();

        assert_eq!(created_restaurant.name, "Test Restaurant");
        assert_eq!(created_restaurant.address, "Test Address");

        let created_menu_items = schema::restaurant_menu_items::table
            .filter(schema::restaurant_menu_items::restaurant_id.eq(created_restaurant.id))
            .load::<models::RestaurantMenuItem>(conn)
            .unwrap();

        assert_eq!(created_menu_items.len(), 2);
        assert!(created_menu_items.iter().any(|item| item.name == "Item 1"));
        assert!(created_menu_items.iter().any(|item| item.name == "Item 2"));
        assert!(created_menu_items
            .iter()
            .any(|item| item.price == BigDecimal::parse_bytes(b"10.00", 10).unwrap()));
        assert!(created_menu_items
            .iter()
            .any(|item| item.price == BigDecimal::parse_bytes(b"20.00", 10).unwrap()));
    }

    #[tokio::test]
    async fn test_create_restaurant_invalid_price() {
        setup_database();

        let service = RestaurantServiceImpl::default();
        let payload = CreateRestaurantPayload {
            name: "Test Restaurant".to_string(),
            address: "Test Address".to_string(),
            menu_items: vec![MenuItem {
                id: "item1".to_string(),
                name: "Item 1".to_string(),
                price: Some(Money {
                    amount: "invalid".to_string(),
                }),
            }],
        };
        let request = Request::new(payload);
        let response = service.create_restaurant(request).await;

        assert!(response.is_err());
        assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_get_restaurant() {
        setup_database();

        let service = RestaurantServiceImpl::default();

        let restaurant_id = Uuid::new_v4();

        let restaurant = models::Restaurant {
            id: restaurant_id,
            name: "Test Restaurant".to_string(),
            address: "Test Address".to_string(),
        };

        let menu_items = vec![
            models::RestaurantMenuItem {
                restaurant_id: restaurant_id,
                id: "item1".to_string(),
                name: "Item 1".to_string(),
                price: BigDecimal::parse_bytes(b"10.00", 10).unwrap(),
            },
            models::RestaurantMenuItem {
                restaurant_id: restaurant_id,
                id: "item2".to_string(),
                name: "Item 2".to_string(),
                price: BigDecimal::parse_bytes(b"20.00", 10).unwrap(),
            },
        ];

        let conn = &mut establish_connection();
        diesel::insert_into(schema::restaurants::table)
            .values(&restaurant)
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::restaurant_menu_items::table)
            .values(&menu_items)
            .execute(conn)
            .unwrap();

        let payload = GetRestaurantPayload {
            restaurant_id: restaurant_id.to_string(),
        };
        let request = Request::new(payload);
        let response = service.get_restaurant(request).await.unwrap();
        let restaurant_response = response.into_inner().restaurant.unwrap();

        assert_eq!(restaurant_response.name, "Test Restaurant");
        assert_eq!(restaurant_response.address, "Test Address");
        assert_eq!(restaurant_response.menu_items.len(), 2);
    }

    #[tokio::test]
    async fn test_get_restaurant_invalid_id() {
        setup_database();

        let service = RestaurantServiceImpl::default();
        let payload = GetRestaurantPayload {
            restaurant_id: "invalid_id".to_string(),
        };
        let request = Request::new(payload);
        let response = service.get_restaurant(request).await;

        assert!(response.is_err());
        assert_eq!(response.unwrap_err().code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn test_list_restaurant() {
        setup_database();
        let service = RestaurantServiceImpl::default();
        let restaurant_id1 = Uuid::new_v4();
        let restaurant_id2 = Uuid::new_v4();
        let restaurant1 = models::Restaurant {
            id: restaurant_id1,
            name: "Test Restaurant 1".to_string(),
            address: "Test Address 1".to_string(),
        };
        let restaurant2 = models::Restaurant {
            id: restaurant_id2,
            name: "Test Restaurant 2".to_string(),
            address: "Test Address 2".to_string(),
        };

        let menu_items1 = vec![models::RestaurantMenuItem {
            restaurant_id: restaurant_id1,
            id: "item1".to_string(),
            name: "Item 1".to_string(),
            price: BigDecimal::parse_bytes(b"10.00", 10).unwrap(),
        }];
        let menu_items2 = vec![models::RestaurantMenuItem {
            restaurant_id: restaurant_id2,
            id: "item3".to_string(),
            name: "Item 3".to_string(),
            price: BigDecimal::parse_bytes(b"30.00", 10).unwrap(),
        }];

        let conn = &mut establish_connection();
        diesel::insert_into(schema::restaurants::table)
            .values(vec![&restaurant1, &restaurant2])
            .execute(conn)
            .unwrap();
        diesel::insert_into(schema::restaurant_menu_items::table)
            .values(
                menu_items1
                    .into_iter()
                    .chain(menu_items2.into_iter())
                    .collect::<Vec<_>>(),
            )
            .execute(conn)
            .unwrap();

        let request = Request::new(());
        let response = service.list_restaurant(request).await.unwrap();
        let restaurants = response.into_inner().restaurants;
        assert!(restaurants.iter().any(|r| r.name == "Test Restaurant 1"));
        assert!(restaurants.iter().any(|r| r.name == "Test Restaurant 2"));
        assert!(restaurants
            .iter()
            .any(|r| r.menu_items.len() == 1 && r.menu_items[0].name == "Item 1"));
        assert!(restaurants
            .iter()
            .any(|r| r.menu_items.len() == 1 && r.menu_items[0].name == "Item 3"));
    }
}
