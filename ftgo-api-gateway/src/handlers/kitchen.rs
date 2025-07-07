use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Router,
};
use ftgo_proto::kitchen_service::{
    AcceptTicketPayload, GetTicketPayload, ListTicketPayload, PreparingTicketPayload,
    ReadyForPickupTicketPayload,
};
use serde::Deserialize;
use tracing::instrument;

use crate::error::ApiError;
use crate::models::*;

use super::{verify_restaurant_access, AppState};
use ftgo_proto::kitchen_service::Ticket;

// Helper function to convert proto Ticket to our KitchenTicket model
fn ticket_to_kitchen_ticket(t: Ticket) -> Result<KitchenTicket, ApiError> {
    Ok(KitchenTicket {
        id: t.id.parse().map_err(|_| ApiError::InvalidToken)?,
        restaurant_id: t
            .restaurant_id
            .parse()
            .map_err(|_| ApiError::InvalidToken)?,
        order_id: None, // Not available in kitchen service proto
        state: match t.state {
            0 => "CREATE_PENDING".to_string(),
            1 => "AWAITING_ACCEPTANCE".to_string(),
            2 => "ACCEPTED".to_string(),
            3 => "PREPARING".to_string(),
            4 => "READY_FOR_PICKUP".to_string(),
            5 => "PICKED_UP".to_string(),
            6 => "CANCEL_PENDING".to_string(),
            7 => "CANCELLED".to_string(),
            _ => "UNKNOWN".to_string(),
        },
        line_items: t
            .line_items
            .into_iter()
            .map(|item| TicketLineItem {
                quantity: item.quantity,
                menu_item_id: item.menu_item_id,
                name: item.name,
            })
            .collect(),
        ready_by: t
            .ready_by
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
        accepted_at: t
            .accept_time
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
        preparing_at: t
            .preparing_time
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
        ready_for_pickup_at: t
            .ready_for_pickup_time
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.seconds, ts.nanos as u32)),
    })
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/restaurants/{restaurant_id}/tickets", get(list_tickets))
        .route(
            "/restaurants/{restaurant_id}/tickets/{ticket_id}",
            get(get_ticket),
        )
        .route(
            "/restaurants/{restaurant_id}/tickets/{ticket_id}/accept",
            post(accept_ticket),
        )
        .route(
            "/restaurants/{restaurant_id}/tickets/{ticket_id}/preparing",
            post(preparing_ticket),
        )
        .route(
            "/restaurants/{restaurant_id}/tickets/{ticket_id}/ready",
            post(ready_for_pickup_ticket),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListTicketsQuery {
    pub first: Option<u32>,
    pub after: Option<String>,
    pub last: Option<u32>,
    pub before: Option<String>,
}

#[utoipa::path(
    get,
    path = "/restaurants/{restaurant_id}/tickets",
    responses(
        (status = 200, description = "List of kitchen tickets", body = ListTicketsResponse),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("restaurant_id" = String, Path, description = "Restaurant ID"),
        ("first" = Option<u32>, Query, description = "Number of items to fetch"),
        ("after" = Option<String>, Query, description = "Cursor for pagination"),
        ("last" = Option<u32>, Query, description = "Number of items to fetch from end"),
        ("before" = Option<String>, Query, description = "Cursor for reverse pagination")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "kitchen"
)]
#[instrument(skip(state))]
pub async fn list_tickets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(restaurant_id): Path<String>,
    Query(query): Query<ListTicketsQuery>,
) -> Result<Json<ListTicketsResponse>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to this restaurant
    verify_restaurant_access(&headers, &mut auth_client, &restaurant_id).await?;

    let mut kitchen_client = state.kitchen_client.clone();

    let request = tonic::Request::new(ListTicketPayload {
        restaurant_id,
        first: query.first,
        after: query.after,
        last: query.last,
        before: query.before,
    });

    let response = kitchen_client
        .list_tickets(request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Kitchen service error: {e}")))?;

    let tickets_response = response.into_inner();
    let tickets = tickets_response
        .edges
        .into_iter()
        .map(|edge| {
            let t = edge.node.ok_or(ApiError::ServiceUnavailable(
                "Invalid ticket data".to_string(),
            ))?;
            ticket_to_kitchen_ticket(t)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ListTicketsResponse { tickets }))
}

#[utoipa::path(
    get,
    path = "/restaurants/{restaurant_id}/tickets/{ticket_id}",
    responses(
        (status = 200, description = "Kitchen ticket details", body = KitchenTicket),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Ticket not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("restaurant_id" = String, Path, description = "Restaurant ID"),
        ("ticket_id" = String, Path, description = "Ticket ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "kitchen"
)]
#[instrument(skip(state))]
pub async fn get_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((restaurant_id, ticket_id)): Path<(String, String)>,
) -> Result<Json<KitchenTicket>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to this restaurant
    verify_restaurant_access(&headers, &mut auth_client, &restaurant_id).await?;

    let mut kitchen_client = state.kitchen_client.clone();

    let request = tonic::Request::new(GetTicketPayload { ticket_id });

    let response = kitchen_client.get_ticket(request).await.map_err(|e| {
        if e.code() == tonic::Code::NotFound {
            ApiError::ServiceUnavailable("Ticket not found".to_string())
        } else {
            ApiError::ServiceUnavailable(format!("Kitchen service error: {e}"))
        }
    })?;

    let ticket = response.into_inner();
    let kitchen_ticket = ticket_to_kitchen_ticket(ticket)?;

    Ok(Json(kitchen_ticket))
}

#[utoipa::path(
    post,
    path = "/restaurants/{restaurant_id}/tickets/{ticket_id}/accept",
    responses(
        (status = 200, description = "Ticket accepted successfully", body = KitchenTicket),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Ticket not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("restaurant_id" = String, Path, description = "Restaurant ID"),
        ("ticket_id" = String, Path, description = "Ticket ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "kitchen"
)]
#[instrument(skip(state))]
pub async fn accept_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((restaurant_id, ticket_id)): Path<(String, String)>,
) -> Result<Json<KitchenTicket>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to this restaurant
    verify_restaurant_access(&headers, &mut auth_client, &restaurant_id).await?;

    let mut kitchen_client = state.kitchen_client.clone();

    let request = tonic::Request::new(AcceptTicketPayload {
        ticket_id: ticket_id.clone(),
        ready_by: None, // No specific ready_by time for now
    });

    kitchen_client.accept_ticket(request).await.map_err(|e| {
        if e.code() == tonic::Code::NotFound {
            ApiError::ServiceUnavailable("Ticket not found".to_string())
        } else {
            ApiError::ServiceUnavailable(format!("Kitchen service error: {e}"))
        }
    })?;

    // After accepting, fetch the updated ticket
    let get_request = tonic::Request::new(GetTicketPayload { ticket_id });
    let response = kitchen_client
        .get_ticket(get_request)
        .await
        .map_err(|e| ApiError::ServiceUnavailable(format!("Kitchen service error: {e}")))?;

    let ticket = response.into_inner();
    let kitchen_ticket = ticket_to_kitchen_ticket(ticket)?;

    Ok(Json(kitchen_ticket))
}

#[utoipa::path(
    post,
    path = "/restaurants/{restaurant_id}/tickets/{ticket_id}/preparing",
    responses(
        (status = 200, description = "Ticket marked as preparing", body = KitchenTicket),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Ticket not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("restaurant_id" = String, Path, description = "Restaurant ID"),
        ("ticket_id" = String, Path, description = "Ticket ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "kitchen"
)]
#[instrument(skip(state))]
pub async fn preparing_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((restaurant_id, ticket_id)): Path<(String, String)>,
) -> Result<Json<KitchenTicket>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to this restaurant
    verify_restaurant_access(&headers, &mut auth_client, &restaurant_id).await?;

    let mut kitchen_client = state.kitchen_client.clone();

    let request = tonic::Request::new(PreparingTicketPayload { ticket_id });

    let response = kitchen_client
        .preparing_ticket(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Ticket not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Kitchen service error: {e}"))
            }
        })?;

    let ticket = response.into_inner();
    let kitchen_ticket = ticket_to_kitchen_ticket(ticket)?;

    Ok(Json(kitchen_ticket))
}

#[utoipa::path(
    post,
    path = "/restaurants/{restaurant_id}/tickets/{ticket_id}/ready",
    responses(
        (status = 200, description = "Ticket marked as ready for pickup", body = KitchenTicket),
        (status = 401, description = "Unauthorized", body = ApiErrorResponse),
        (status = 404, description = "Ticket not found", body = ApiErrorResponse),
        (status = 503, description = "Service unavailable", body = ApiErrorResponse),
    ),
    params(
        ("restaurant_id" = String, Path, description = "Restaurant ID"),
        ("ticket_id" = String, Path, description = "Ticket ID")
    ),
    security(
        ("bearer" = [])
    ),
    tag = "kitchen"
)]
#[instrument(skip(state))]
pub async fn ready_for_pickup_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((restaurant_id, ticket_id)): Path<(String, String)>,
) -> Result<Json<KitchenTicket>, ApiError> {
    let mut auth_client = state.auth_client.clone();

    // Verify user has access to this restaurant
    verify_restaurant_access(&headers, &mut auth_client, &restaurant_id).await?;

    let mut kitchen_client = state.kitchen_client.clone();

    let request = tonic::Request::new(ReadyForPickupTicketPayload { ticket_id });

    let response = kitchen_client
        .ready_for_pickup_ticket(request)
        .await
        .map_err(|e| {
            if e.code() == tonic::Code::NotFound {
                ApiError::ServiceUnavailable("Ticket not found".to_string())
            } else {
                ApiError::ServiceUnavailable(format!("Kitchen service error: {e}"))
            }
        })?;

    let ticket = response.into_inner();
    let kitchen_ticket = ticket_to_kitchen_ticket(ticket)?;

    Ok(Json(kitchen_ticket))
}
