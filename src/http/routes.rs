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
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(core)
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok"}))
}

async fn not_found() -> ApiError {
    ApiError::new(axum::http::StatusCode::NOT_FOUND, "not found")
}

async fn method_not_allowed() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::METHOD_NOT_ALLOWED,
        "method not allowed",
    )
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
            Path(id): Path<String>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::ApiFuture;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use hmac::{Hmac, Mac};
    use serde_json::json;
    use sha2::Sha256;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    #[derive(Clone)]
    struct StubCore {
        secret: &'static str,
        response: ApiResponse,
        seen: Arc<Mutex<Vec<ApiRequest>>>,
    }

    impl StubCore {
        fn ok() -> Self {
            Self::with_response(ApiResponse {
                status: StatusCode::OK,
                body: json!({"ok":true}),
                headers: HeaderMap::new(),
                raw_body: None,
            })
        }

        fn with_response(response: ApiResponse) -> Self {
            Self {
                secret: "test-secret",
                response,
                seen: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn seen(&self) -> Vec<ApiRequest> {
            self.seen.lock().unwrap().clone()
        }
    }

    impl HttpCore for StubCore {
        fn dispatch(&self, request: ApiRequest) -> ApiFuture {
            let seen = self.seen.clone();
            let response = self.response.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(request);
                Ok(response)
            })
        }

        fn bank_secret(&self) -> &str {
            self.secret
        }
    }

    #[derive(Clone)]
    struct FailingCore;

    impl HttpCore for FailingCore {
        fn dispatch(&self, _request: ApiRequest) -> ApiFuture {
            Box::pin(async { Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden")) })
        }

        fn bank_secret(&self) -> &str {
            "test-secret"
        }
    }

    async fn send<C: HttpCore>(
        core: C,
        request: Request<Body>,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let response = router(Arc::new(core)).oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        (status, headers, body)
    }

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        mac.finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn json_body(body: &[u8]) -> Value {
        serde_json::from_slice(body).unwrap()
    }

    fn is_json(headers: &HeaderMap) -> bool {
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"))
    }

    #[tokio::test]
    async fn health_route_returns_json_without_calling_core() {
        let core = StubCore::ok();
        let (status, headers, body) = send(
            core.clone(),
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(is_json(&headers));
        assert_eq!(json_body(&body), json!({"status":"ok"}));
        assert!(core.seen().is_empty());
    }

    #[tokio::test]
    async fn public_products_route_forwards_without_bearer() {
        let core = StubCore::ok();
        let (status, _, _) = send(
            core.clone(),
            Request::builder()
                .uri("/api/products?q=keyboard&sort=price")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let seen = core.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].path, "/api/products");
        assert_eq!(seen[0].query.get("q").map(String::as_str), Some("keyboard"));
        assert_eq!(seen[0].query.get("sort").map(String::as_str), Some("price"));
        assert!(seen[0].bearer.is_none());
    }

    #[tokio::test]
    async fn protected_route_without_bearer_returns_json_401_and_skips_core() {
        let core = StubCore::ok();
        let (status, headers, body) = send(
            core.clone(),
            Request::builder()
                .uri("/api/users/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(is_json(&headers));
        assert_eq!(json_body(&body), json!({"error":"unauthorized"}));
        assert!(core.seen().is_empty());
    }

    #[tokio::test]
    async fn protected_route_with_bearer_forwards_normalized_request() {
        let core = StubCore::ok();
        let (status, _, _) = send(
            core.clone(),
            Request::builder()
                .uri("/api/users/me")
                .header(header::AUTHORIZATION, "bEaReR token-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let seen = core.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].path, "/api/users/me");
        assert_eq!(seen[0].bearer.as_deref(), Some("token-1"));
    }

    #[tokio::test]
    async fn invalid_bearer_syntax_returns_json_401() {
        for authorization in ["Basic abc", "Bearer", "Bearer "] {
            let core = StubCore::ok();
            let (status, headers, body) = send(
                core.clone(),
                Request::builder()
                    .uri("/api/users/me")
                    .header(header::AUTHORIZATION, authorization)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "header: {authorization}");
            assert!(is_json(&headers));
            assert_eq!(json_body(&body), json!({"error":"unauthorized"}));
            assert!(core.seen().is_empty());
        }
    }

    #[tokio::test]
    async fn core_error_is_serialized_as_json_response() {
        let (status, headers, body) = send(
            FailingCore,
            Request::builder()
                .uri("/api/wallet")
                .header(header::AUTHORIZATION, "Bearer token")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(is_json(&headers));
        assert_eq!(json_body(&body), json!({"error":"forbidden"}));
    }

    #[tokio::test]
    async fn raw_core_response_preserves_bytes_and_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, "application/gzip".parse().unwrap());
        let core = StubCore::with_response(ApiResponse {
            status: StatusCode::OK,
            body: Value::Null,
            headers,
            raw_body: Some(vec![0x1f, 0x8b, 0x08]),
        });
        let (status, headers, body) = send(
            core,
            Request::builder()
                .uri("/api/reports/download?file=sales.csv.gz")
                .header(header::AUTHORIZATION, "Bearer token")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get(header::CONTENT_TYPE).unwrap(),
            "application/gzip"
        );
        assert_eq!(body, vec![0x1f, 0x8b, 0x08]);
    }

    #[tokio::test]
    async fn unknown_route_and_wrong_method_return_json_errors() {
        let core = StubCore::ok();
        let (status, headers, body) = send(
            core.clone(),
            Request::builder()
                .uri("/api/nope")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(is_json(&headers));
        assert_eq!(json_body(&body), json!({"error":"not found"}));

        let (status, headers, body) = send(
            core,
            Request::builder()
                .method("PUT")
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert!(is_json(&headers));
        assert_eq!(json_body(&body), json!({"error":"method not allowed"}));
    }

    #[tokio::test]
    async fn valid_webhook_signature_dispatches_decoded_body() {
        let core = StubCore::ok();
        let payload = br#"{"order_id":7,"status":"settled"}"#;
        let signature = sign(core.bank_secret(), payload);
        let (status, _, _) = send(
            core.clone(),
            Request::builder()
                .method("POST")
                .uri("/api/webhooks/bank")
                .header("x-signature", signature)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_vec()))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let seen = core.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].path, "/api/webhooks/bank");
        assert_eq!(seen[0].body, json!({"order_id":7,"status":"settled"}));
    }

    #[tokio::test]
    async fn invalid_webhook_signatures_are_rejected_without_dispatch() {
        let payload = br#"{"order_id":7,"status":"settled"}"#;
        let cases = [None, Some("not-hex"), Some("0"), Some("deadbeef")];
        for signature in cases {
            let core = StubCore::ok();
            let mut builder = Request::builder()
                .method("POST")
                .uri("/api/webhooks/bank")
                .header(header::CONTENT_TYPE, "application/json");
            if let Some(signature) = signature {
                builder = builder.header("x-signature", signature);
            }
            let (status, headers, body) = send(
                core.clone(),
                builder.body(Body::from(payload.to_vec())).unwrap(),
            )
            .await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "signature: {signature:?}");
            assert!(is_json(&headers));
            assert_eq!(json_body(&body), json!({"error":"bad signature"}));
            assert!(core.seen().is_empty());
        }
    }

    #[test]
    fn protected_paths_are_classified_explicitly() {
        for path in [
            "/health",
            "/api/register",
            "/api/login",
            "/api/webhooks/bank",
            "/api/products",
            "/api/products/1",
        ] {
            assert!(!protected(path), "expected public path: {path}");
        }
        for path in [
            "/api/logout",
            "/api/users/me",
            "/api/orders",
            "/api/orders/1",
            "/api/wallet",
            "/api/reports/sales",
            "/api/admin/users",
        ] {
            assert!(protected(path), "expected protected path: {path}");
        }
    }
}
