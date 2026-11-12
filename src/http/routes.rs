use std::{collections::BTreeMap, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use super::{
    error::{ApiError, ApiResult},
    models::{ApiRequest, ApiResponse, HttpCore, HttpMethod},
};

pub type HttpState<C> = Arc<C>;

/// Build the complete public route table.  The core adapter is shared by all
/// handlers, which keeps the HTTP contract stable while domain code evolves.
pub fn router<C: HttpCore>(core: HttpState<C>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/register", post(register::<C>))
        .route("/api/login", post(login::<C>))
        .route("/api/logout", post(logout::<C>))
        .route("/api/users/me", get(me::<C>).patch(update_me::<C>))
        .route(
            "/api/products",
            get(products::<C>).post(create_product::<C>),
        )
        .route(
            "/api/products/:id",
            get(product::<C>).put(update_product::<C>),
        )
        .route("/api/orders", get(orders::<C>).post(create_order::<C>))
        .route("/api/orders/:id", get(order::<C>))
        .route("/api/orders/:id/invoice", get(invoice::<C>))
        .route("/api/orders/:id/pay", post(pay::<C>))
        .route("/api/orders/:id/cancel", post(cancel::<C>))
        .route("/api/orders/:id/refund", post(refund::<C>))
        .route("/api/wallet", get(wallet::<C>))
        .route("/api/wallet/topup", post(topup::<C>))
        .route("/api/wallet/transfer", post(transfer::<C>))
        .route("/api/reports/sales", get(sales_report::<C>))
        .route("/api/reports/export", post(export_report::<C>))
        .route("/api/reports/download", get(download_report::<C>))
        .route("/api/admin/users", get(admin_users::<C>))
        .route("/api/admin/users/:id/role", post(set_role::<C>))
        .route("/api/admin/orders/:id/status", post(set_order_status::<C>))
        .route("/api/webhooks/bank", post(bank_webhook::<C>))
        .with_state(core)
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

fn protected(path: &str) -> bool {
    if matches!(
        path,
        "/health" | "/api/register" | "/api/login" | "/api/webhooks/bank"
    ) {
        return false;
    }
    !(path == "/api/products" || path.starts_with("/api/products/"))
}

fn bearer(headers: &HeaderMap) -> ApiResult<Option<String>> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let text = value.to_str().map_err(|_| ApiError::unauthorized())?;
    let mut parts = text.splitn(2, ' ');
    if !parts
        .next()
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
    {
        return Err(ApiError::unauthorized());
    }
    let token = parts
        .next()
        .filter(|v| !v.is_empty())
        .ok_or_else(ApiError::unauthorized)?;
    Ok(Some(token.to_owned()))
}

fn query_map(query: Query<BTreeMap<String, String>>) -> BTreeMap<String, String> {
    query.0
}

async fn dispatch<C: HttpCore>(
    state: State<HttpState<C>>,
    method: HttpMethod,
    path: String,
    query: BTreeMap<String, String>,
    headers: HeaderMap,
    body: Value,
) -> Response {
    let token = match bearer(&headers) {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    if protected(&path) && token.is_none() {
        return ApiError::unauthorized().into_response();
    }
    let result = state
        .dispatch(ApiRequest {
            method,
            path,
            query,
            body,
            bearer: token,
        })
        .await;
    match result {
        Ok(value) => value.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn body<C: HttpCore>(
    state: State<HttpState<C>>,
    method: HttpMethod,
    path: String,
    query: Query<BTreeMap<String, String>>,
    headers: HeaderMap,
    payload: Option<Json<Value>>,
) -> Response {
    dispatch(
        state,
        method,
        path,
        query_map(query),
        headers,
        payload.map(|x| x.0).unwrap_or_else(|| json!({})),
    )
    .await
}

macro_rules! json_handler {
    ($name:ident, $method:ident, $path:expr) => {
        async fn $name<C: HttpCore>(
            State(state): State<HttpState<C>>,
            Query(query): Query<BTreeMap<String, String>>,
            headers: HeaderMap,
            payload: Option<Json<Value>>,
        ) -> Response {
            let path = $path.to_owned();
            body(
                State(state),
                HttpMethod::$method,
                path,
                Query(query),
                headers,
                payload,
            )
            .await
        }
    };
}

macro_rules! json_id_handler {
    ($name:ident, $method:ident, $path:expr) => {
        async fn $name<C: HttpCore>(
            Path(id): Path<u64>,
            State(state): State<HttpState<C>>,
            Query(query): Query<BTreeMap<String, String>>,
            headers: HeaderMap,
            payload: Option<Json<Value>>,
        ) -> Response {
            let path = format!($path, id);
            body(
                State(state),
                HttpMethod::$method,
                path,
                Query(query),
                headers,
                payload,
            )
            .await
        }
    };
}

// Axum route handlers intentionally use the same normalized adapter for all
// resources. This prevents individual handlers from drifting in error shape.
json_handler!(register, Post, "/api/register");
json_handler!(login, Post, "/api/login");
json_handler!(logout, Post, "/api/logout");
json_handler!(me, Get, "/api/users/me");
json_handler!(update_me, Patch, "/api/users/me");
json_handler!(products, Get, "/api/products");
json_handler!(create_product, Post, "/api/products");
json_id_handler!(product, Get, "/api/products/{}");
json_id_handler!(update_product, Put, "/api/products/{}");
json_handler!(orders, Get, "/api/orders");
json_handler!(create_order, Post, "/api/orders");
json_id_handler!(order, Get, "/api/orders/{}");
json_id_handler!(invoice, Get, "/api/orders/{}/invoice");
json_id_handler!(pay, Post, "/api/orders/{}/pay");
json_id_handler!(cancel, Post, "/api/orders/{}/cancel");
json_id_handler!(refund, Post, "/api/orders/{}/refund");
json_handler!(wallet, Get, "/api/wallet");
json_handler!(topup, Post, "/api/wallet/topup");
json_handler!(transfer, Post, "/api/wallet/transfer");
json_handler!(sales_report, Get, "/api/reports/sales");
json_handler!(export_report, Post, "/api/reports/export");
json_handler!(download_report, Get, "/api/reports/download");
json_handler!(admin_users, Get, "/api/admin/users");
json_id_handler!(set_role, Post, "/api/admin/users/{}/role");
json_id_handler!(set_order_status, Post, "/api/admin/orders/{}/status");

async fn bank_webhook<C: HttpCore>(
    State(state): State<HttpState<C>>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Response {
    let secret = state.bank_secret();
    let Some(signature) = headers.get("x-signature").and_then(|v| v.to_str().ok()) else {
        return ApiError::bad_signature().into_response();
    };
    if !crate::domain::verify_webhook_signature(secret, &bytes, signature) {
        return ApiError::bad_signature().into_response();
    }
    let body = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => json!({}),
    };
    let request = ApiRequest {
        method: HttpMethod::Post,
        path: "/api/webhooks/bank".into(),
        query: BTreeMap::new(),
        body,
        bearer: None,
    };
    match state.dispatch(request).await {
        Ok(v) => v.into_response(),
        Err(e) => e.into_response(),
    }
}

impl IntoResponse for ApiResponse {
    fn into_response(self) -> Response {
        if let Some(body) = self.raw_body {
            let mut response = (self.status, body).into_response();
            *response.headers_mut() = self.headers;
            return response;
        }
        let mut response = (self.status, Json(self.body)).into_response();
        *response.headers_mut() = self.headers;
        response
    }
}
