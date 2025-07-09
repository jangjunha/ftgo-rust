use std::env;

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Argon2, PasswordHash};
use chrono::{TimeDelta, Utc};
use diesel::{insert_into, prelude::*, result::Error::NotFound};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use ftgo_auth_service::models::{
    UserConsumerGrants, UserCourierGrants, UserCredentials, UserRestaurantGrants,
};
use ftgo_proto::auth_service::auth_service_server::{AuthService, AuthServiceServer};
use ftgo_proto::auth_service::{
    CreateUserPayload, CredentialType, GetTokenInfoPayload, GetUserPayload,
    GrantConsumerToUserPayload, GrantCourierToUserPayload, GrantRestaurantToUserPayload,
    IssueTokenPayload, TokenInfo, TokenResponse, User,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use prost_types::Timestamp;
use serde::{Deserialize, Serialize};
use tonic::{transport::Server, Request, Response, Status};
use uuid::Uuid;

use ftgo_auth_service::{establish_connection, models, schema};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations");

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    exp: usize,
    iat: usize,
    sub: String,
}

pub struct AuthServiceImpl {
    pub encoding_key: EncodingKey,
    pub decoding_key: DecodingKey,
    pub access_token_expires: TimeDelta,
}

impl AuthServiceImpl {
    pub fn new() -> Self {
        let secret_key = env::var("SECRET_KEY").expect("SECRET_KEY must be set");
        Self {
            encoding_key: EncodingKey::from_secret(secret_key.as_ref()),
            decoding_key: DecodingKey::from_secret(secret_key.as_ref()),
            access_token_expires: TimeDelta::hours(8),
        }
    }
}

#[tonic::async_trait]
impl AuthService for AuthServiceImpl {
    async fn create_user(
        &self,
        request: Request<CreateUserPayload>,
    ) -> Result<Response<User>, Status> {
        let argon2 = Argon2::default();

        let salt = SaltString::generate(&mut OsRng);
        let payload = request.into_inner();
        let user = models::User {
            id: Uuid::new_v4(),
            username: payload.username,
            created_at: Utc::now(),
        };
        let user_credential = models::UserCredentials {
            user_id: user.id,
            credential_type: models::CredentialType::Passphrase,
            sub: argon2
                .hash_password(payload.passphrase.as_bytes(), &salt)
                .map_err(|_| Status::internal("message"))?
                .to_string(),
        };

        let conn = &mut establish_connection();
        conn.transaction(|conn| {
            insert_into(schema::users::table)
                .values(&user)
                .execute(conn)?;
            insert_into(schema::user_credentials::table)
                .values(&user_credential)
                .execute(conn)?;
            Ok(())
        })
        .map_err(|_: diesel::result::Error| Status::internal("Failed to create user"))?;

        Ok(Response::new(User {
            id: user.id.to_string(),
            username: user.username,
            created_at: Some(Timestamp {
                seconds: user.created_at.timestamp(),
                nanos: user.created_at.timestamp_subsec_nanos() as i32,
            }),
            granted_restaurants: vec![],
            granted_consumers: vec![],
            granted_couriers: vec![],
        }))
    }

    async fn get_user(&self, request: Request<GetUserPayload>) -> Result<Response<User>, Status> {
        let payload = request.into_inner();
        let id =
            Uuid::parse_str(&payload.id).map_err(|_| Status::invalid_argument("Invalid id"))?;

        let conn = &mut establish_connection();
        let (user, restaurant_grants, consumer_grants, courier_grants) = conn
            .transaction(|conn| {
                let user = schema::users::table
                    .select(models::User::as_select())
                    .find(&id)
                    .first::<models::User>(conn)?;
                let restaurant_grants = models::UserRestaurantGrants::belonging_to(&user)
                    .select(UserRestaurantGrants::as_select())
                    .load(conn)?;
                let consumer_grants = models::UserConsumerGrants::belonging_to(&user)
                    .select(UserConsumerGrants::as_select())
                    .load(conn)?;
                let courier_grants = models::UserCourierGrants::belonging_to(&user)
                    .select(UserCourierGrants::as_select())
                    .load(conn)?;
                Ok((user, restaurant_grants, consumer_grants, courier_grants))
            })
            .map_err(|err| match err {
                NotFound => Status::not_found("User not found"),
                _ => Status::internal("Cannot retrieve user"),
            })?;

        Ok(Response::new(User {
            id: user.id.to_string(),
            username: user.username,
            created_at: Some(Timestamp {
                seconds: user.created_at.timestamp(),
                nanos: user.created_at.timestamp_subsec_nanos() as i32,
            }),
            granted_restaurants: restaurant_grants
                .iter()
                .map(|g| g.restaurant_id.to_string())
                .collect(),
            granted_consumers: consumer_grants
                .iter()
                .map(|g| g.consumer_id.to_string())
                .collect(),
            granted_couriers: courier_grants
                .iter()
                .map(|g| g.courier_id.to_string())
                .collect(),
        }))
    }

    async fn issue_token(
        &self,
        request: Request<IssueTokenPayload>,
    ) -> Result<Response<TokenResponse>, Status> {
        let payload = request.into_inner();
        let conn = &mut establish_connection();

        let invalid_credentials = || Status::invalid_argument("Invalid credentials");
        match CredentialType::try_from(payload.credential_type)
            .map_err(|_| Status::invalid_argument("Invalid credential_type"))?
        {
            CredentialType::Passphrase => {
                let argon2 = Argon2::default();
                let username = payload.username.ok_or(Status::invalid_argument(
                    "username required for passphrase credentials",
                ))?;
                let user = schema::users::table
                    .select(models::User::as_select())
                    .filter(schema::users::username.eq(username))
                    .first::<models::User>(conn)
                    .map_err(|_| invalid_credentials())?;
                let credentials = models::UserCredentials::belonging_to(&user)
                    .select(UserCredentials::as_select())
                    .filter(
                        schema::user_credentials::credential_type
                            .eq(models::CredentialType::Passphrase),
                    )
                    .load(conn)
                    .map_err(|_| Status::internal("Cannot retrieve credentials"))?;
                let verified = credentials
                    .iter()
                    .filter_map(|c| PasswordHash::new(&c.sub).ok())
                    .any(|hash| {
                        argon2
                            .verify_password(payload.sub.as_bytes(), &hash)
                            .is_ok()
                    });
                if verified {
                    let now = Utc::now();
                    let claims = Claims {
                        exp: (now + self.access_token_expires).timestamp() as usize,
                        iat: now.timestamp() as usize,
                        sub: user.id.to_string(),
                    };
                    let access_token = jsonwebtoken::encode(
                        &jsonwebtoken::Header::default(),
                        &claims,
                        &self.encoding_key,
                    )
                    .map_err(|_| Status::internal("Cannot issue token"))?;
                    Ok(Response::new(TokenResponse {
                        token_type: "bearer".to_string(),
                        access_token: access_token,
                        expires_in: self.access_token_expires.num_seconds(),
                    }))
                } else {
                    Err(invalid_credentials())
                }
            }
        }
    }

    async fn get_token_info(
        &self,
        request: Request<GetTokenInfoPayload>,
    ) -> Result<Response<TokenInfo>, Status> {
        let payload = request.into_inner();

        let token = jsonwebtoken::decode::<Claims>(
            &payload.token,
            &self.decoding_key,
            &jsonwebtoken::Validation::default(),
        )
        .map_err(|_| Status::invalid_argument("Invalid token"))?;

        Ok(Response::new(TokenInfo {
            user_id: token.claims.sub.to_string(),
        }))
    }

    async fn grant_restaurant_to_user(
        &self,
        request: Request<GrantRestaurantToUserPayload>,
    ) -> Result<Response<()>, Status> {
        let payload = request.into_inner();
        let grant = models::UserRestaurantGrants {
            user_id: Uuid::parse_str(&payload.user_id)
                .map_err(|_| Status::invalid_argument("Invalid user_id"))?,
            restaurant_id: Uuid::parse_str(&payload.restaurant_id)
                .map_err(|_| Status::invalid_argument("Invalid restaurant_id"))?,
        };

        let conn = &mut establish_connection();
        insert_into(schema::user_restaurant_grants::table)
            .values(&grant)
            .execute(conn)
            .map_err(|_| Status::internal("Failed to grant"))?;

        Ok(Response::new(()))
    }

    async fn grant_consumer_to_user(
        &self,
        request: Request<GrantConsumerToUserPayload>,
    ) -> Result<Response<()>, Status> {
        let payload = request.into_inner();
        let grant = models::UserConsumerGrants {
            user_id: Uuid::parse_str(&payload.user_id)
                .map_err(|_| Status::invalid_argument("Invalid user_id"))?,
            consumer_id: Uuid::parse_str(&payload.consumer_id)
                .map_err(|_| Status::invalid_argument("Invalid consumer_id"))?,
        };

        let conn = &mut establish_connection();
        insert_into(schema::user_consumer_grants::table)
            .values(&grant)
            .execute(conn)
            .map_err(|_| Status::internal("Failed to grant"))?;

        Ok(Response::new(()))
    }

    async fn grant_courier_to_user(
        &self,
        request: Request<GrantCourierToUserPayload>,
    ) -> Result<Response<()>, Status> {
        let payload = request.into_inner();
        let grant = models::UserCourierGrants {
            user_id: Uuid::parse_str(&payload.user_id)
                .map_err(|_| Status::invalid_argument("Invalid user_id"))?,
            courier_id: Uuid::parse_str(&payload.courier_id)
                .map_err(|_| Status::invalid_argument("Invalid courier_id"))?,
        };

        let conn = &mut establish_connection();
        insert_into(schema::user_courier_grants::table)
            .values(&grant)
            .execute(conn)
            .map_err(|_| Status::internal("Failed to grant"))?;

        Ok(Response::new(()))
    }
}

pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = establish_connection();
    conn.run_pending_migrations(MIGRATIONS)
        .expect("Failed to run migrations");

    let addr = "0.0.0.0:8199".parse().unwrap();
    let service = AuthServiceImpl::new();

    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<AuthServiceServer<AuthServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(AuthServiceServer::new(service))
        .serve(addr)
        .await?;

    Ok(())
}
