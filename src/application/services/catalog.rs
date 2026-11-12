use super::{
    ok,
    shared::{int, product_json},
    ShopCore,
};
use crate::http::{ApiError, ApiRequest, ApiResponse};
use crate::repository::products::{self, ProductSort};
use axum::http::StatusCode;
use serde_json::Value;

impl ShopCore {
    pub(crate) async fn products(&self, req: &ApiRequest) -> Result<ApiResponse, ApiError> {
        let q = req.query.get("q").map(String::as_str);
        let sort = match req.query.get("sort").map(String::as_str) {
            Some("price") => ProductSort::Price,
            Some("name") => ProductSort::Name,
            Some("sku") => ProductSort::Sku,
            Some("stock") => ProductSort::Stock,
            _ => ProductSort::Id,
        };
        let page = int(&req.query, "page", 1).max(1);
        let limit = int(&req.query, "limit", 20);
        let rows = products::list(&self.context.pool, q, sort, page, limit)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(Value::Array(rows.iter().map(product_json).collect()))
    }
}
