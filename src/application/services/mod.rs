mod catalog;
mod dispatch;
mod orders;
mod payments;
mod reports;
mod shared;
mod wallet;

use crate::application::ApplicationContext;
use crate::{
    config::Config,
    http::{ApiFuture, ApiRequest, ApiResponse, HttpCore},
};
use anyhow::Result;
use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;

#[derive(Clone)]
pub struct ShopCore {
    pub(crate) context: ApplicationContext,
}

impl ShopCore {
    pub fn new(pool: sqlx::PgPool, config: Config) -> Self {
        Self {
            context: ApplicationContext::new(pool, config),
        }
    }
}

impl HttpCore for ShopCore {
    fn dispatch(&self, request: ApiRequest) -> ApiFuture {
        let service = self.clone();
        Box::pin(async move { service.dispatch_inner(request).await })
    }

    fn bank_secret(&self) -> &str {
        &self.context.config.bank_secret
    }
}

pub(crate) fn ok(body: Value) -> Result<ApiResponse, crate::http::ApiError> {
    Ok(ApiResponse {
        status: StatusCode::OK,
        body,
        headers: HeaderMap::new(),
        raw_body: None,
    })
}

pub(crate) fn created(body: Value) -> Result<ApiResponse, crate::http::ApiError> {
    Ok(ApiResponse {
        status: StatusCode::CREATED,
        body,
        headers: HeaderMap::new(),
        raw_body: None,
    })
}

pub(crate) fn err(
    status: StatusCode,
    message: impl Into<String>,
) -> Result<ApiResponse, crate::http::ApiError> {
    Err(crate::http::ApiError::new(status, message))
}
