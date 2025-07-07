use axum::{extract::{State, Path}, http::HeaderMap, response::Json, Router, routing::get};
use ftgo_proto::accounting_service::GetAccountPayload;
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{AppState, verify_consumer_access};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/consumers/{consumer_id}/account", get(get_account))
}

#[utoipa::path(
    get,
    path = "/consumers/{consumer_id}/account",
    responses(
        (status = 200, description = "Account details", body = AccountDetailsResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Account not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("consumer_id" = String, Path, description = "Consumer ID (equals Account ID)")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "accounting"
)]
#[instrument(skip(state))]
pub async fn get_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(consumer_id): Path<String>,
) -> Result<Json<AccountDetailsResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();
    
    // Verify user has access to this consumer (since account_id = consumer_id)
    verify_consumer_access(&headers, &mut auth_client, &consumer_id).await?;

    let mut accounting_client = state.accounting_client.clone();

    // Account ID equals Consumer ID
    let request = tonic::Request::new(GetAccountPayload { 
        account_id: consumer_id.clone() 
    });

    let response = accounting_client
        .get_account(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Account not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Accounting service error: {e}"))
            }
        })?;

    let account = response.into_inner();

    Ok(Json(AccountDetailsResponse {
        account_id: account.id.parse().map_err(|_| ApiError::InvalidToken)?,
        consumer_id: consumer_id.parse().map_err(|_| ApiError::InvalidToken)?,
        balance: account.balance.map(|b| b.amount).unwrap_or_default(),
    }))
}