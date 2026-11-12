use std::{collections::BTreeMap, future::Future, pin::Pin};

use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;

#[derive(Debug, Clone, Copy)]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
}

#[derive(Debug, Clone)]
pub struct ApiRequest {
    pub method: HttpMethod,
    pub path: String,
    pub query: BTreeMap<String, String>,
    pub body: Value,
    pub bearer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: StatusCode,
    pub body: Value,
    pub headers: HeaderMap,
    pub raw_body: Option<Vec<u8>>,
}

pub type ApiFuture = Pin<Box<dyn Future<Output = Result<ApiResponse, super::ApiError>> + Send>>;

/// Minimal adapter required from the shared domain/repository implementation.
///
/// `path` is the normalized public API path (including `/api`). The adapter is
/// responsible for database transactions, legacy business rules, and response
/// JSON. HTTP authentication syntax is normalized here, while role checks stay
/// in the core because only the core can load the user.
pub trait HttpCore: Send + Sync + 'static {
    fn dispatch(&self, request: ApiRequest) -> ApiFuture;
    fn bank_secret(&self) -> &str;
}
