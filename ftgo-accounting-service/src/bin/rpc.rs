use bigdecimal::BigDecimal;
use ftgo_accounting_service::{
    projection::establish_connection, service::AccountingService, store::AccountStore,
};
use ftgo_proto::{
    accounting_service::{
        accounting_service_server::{
            AccountingService as AccountingServiceBase, AccountingServiceServer,
        },
        AccountDetails, AccountInfo, DepositAccountPayload, GetAccountPayload, ListAccountsPayload,
        ListAccountsResponse, WithdrawAccountPayload,
    },
    common::Money,
};
use std::str::FromStr;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Default)]
struct AccountingServiceImpl {}

#[tonic::async_trait]
impl AccountingServiceBase for AccountingServiceImpl {
    async fn get_account(
        &self,
        request: Request<GetAccountPayload>,
    ) -> Result<Response<AccountDetails>, Status> {
        let payload = request.into_inner();
        let account_id = Uuid::from_str(&payload.account_id)
            .map_err(|_| Status::invalid_argument("Invalid account_id"))?;

        let store = AccountStore::default();
        let conn = &mut establish_connection().await;
        let mut service = AccountingService::new(store, conn);

        let account = service
            .get_account(&account_id)
            .await
            .map_err(|_| Status::internal("Internal error"))?;
        match account {
            Some(account) => Ok(Response::new(AccountDetails {
                id: account.id.to_string(),
                balance: Some(Money {
                    amount: account.amount.to_string(),
                }),
            })),
            None => Err(Status::not_found("Account not found")),
        }
    }

    async fn deposit_account(
        &self,
        request: Request<DepositAccountPayload>,
    ) -> Result<Response<AccountDetails>, Status> {
        let payload = request.into_inner();
        let account_id = Uuid::from_str(&payload.account_id)
            .map_err(|_| Status::invalid_argument("Invalid account_id"))?;
        let amount = payload
            .amount
            .ok_or(Status::invalid_argument("amount must be set"))?
            .amount
            .parse::<BigDecimal>()
            .map_err(|_| Status::invalid_argument("Invalid amount"))?;

        let store = AccountStore::default();
        let conn = &mut establish_connection().await;
        let service = AccountingService::new(store, conn);

        let account = service
            .deposit(account_id, amount, None, None, None)
            .await
            .map_err(|_| Status::internal("Internal error"))?;

        Ok(Response::new(AccountDetails {
            id: account.id.to_string(),
            balance: Some(Money {
                amount: account.balance.to_string(),
            }),
        }))
    }

    async fn withdraw_account(
        &self,
        request: Request<WithdrawAccountPayload>,
    ) -> Result<Response<AccountDetails>, Status> {
        let payload = request.into_inner();
        let account_id = Uuid::from_str(&payload.account_id)
            .map_err(|_| Status::invalid_argument("Invalid account_id"))?;
        let amount = payload
            .amount
            .ok_or(Status::invalid_argument("amount must be set"))?
            .amount
            .parse::<BigDecimal>()
            .map_err(|_| Status::invalid_argument("Invalid amount"))?;

        let store = AccountStore::default();
        let conn = &mut establish_connection().await;
        let service = AccountingService::new(store, conn);

        let account = service
            .withdraw(account_id, amount, None, None, None)
            .await
            .map_err(|_| Status::internal("Internal error"))?;

        Ok(Response::new(AccountDetails {
            id: account.id.to_string(),
            balance: Some(Money {
                amount: account.balance.to_string(),
            }),
        }))
    }

    async fn list_accounts(
        &self,
        request: Request<ListAccountsPayload>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        let payload = request.into_inner();

        let store = AccountStore::default();
        let conn = &mut establish_connection().await;
        let mut service = AccountingService::new(store, conn);

        let account_infos = service
            .list_accounts(payload.page_number, payload.page_size)
            .await
            .map_err(|_| Status::internal("Internal error"))?;

        Ok(Response::new(ListAccountsResponse {
            accounts: account_infos
                .into_iter()
                .map(|i| AccountInfo {
                    id: i.id.to_string(),
                    deposit_accumulate: Some(Money {
                        amount: i.deposit_accumulate.to_string(),
                    }),
                    deposit_count: i.deposit_count,
                    withdraw_accumulate: Some(Money {
                        amount: i.withdraw_accumulate.to_string(),
                    }),
                    withdraw_count: i.withdraw_count,
                })
                .collect(),
        }))
    }
}

#[tokio::main]
pub async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "0.0.0.0:8104".parse().unwrap();

    let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<AccountingServiceServer<AccountingServiceImpl>>()
        .await;

    println!("listening on {}", addr);

    Server::builder()
        .add_service(health_service)
        .add_service(AccountingServiceServer::new(
            AccountingServiceImpl::default(),
        ))
        .serve(addr)
        .await?;

    Ok(())
}
