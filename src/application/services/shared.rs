use super::ShopCore;
use crate::{
    http::{ApiError, ApiRequest},
    repository,
};
use axum::http::StatusCode;
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub(crate) fn id(path: &str, marker: &str) -> Option<i32> {
    path.strip_prefix(marker)?.split('/').next()?.parse().ok()
}
pub(crate) fn int(q: &BTreeMap<String, String>, k: &str, d: i64) -> i64 {
    q.get(k).and_then(|x| x.parse().ok()).unwrap_or(d)
}
pub(crate) fn money(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64)
        .or_else(|| v.and_then(Value::as_i64).map(|x| x as f64))
        .unwrap_or(0.0)
}
pub(crate) fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

pub(crate) fn legacy_datetime(value: Option<chrono::NaiveDateTime>) -> Option<String> {
    value.map(|value| format!("{} GMT", value.format("%a, %d %b %Y %H:%M:%S")))
}

pub(crate) fn product_json(product: &crate::repository::products::ProductRecord) -> Value {
    json!({
        "id": product.id,
        "sku": product.sku,
        "name": product.name,
        "price": product.price.unwrap_or(0.0),
        "stock": product.stock.unwrap_or(0),
        "created_at": legacy_datetime(product.created_at)
    })
}

pub(crate) fn report_date(value: &str) -> Result<chrono::NaiveDate, ApiError> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid date"))
}

impl ShopCore {
    pub(crate) async fn actor(&self, req: &ApiRequest) -> Result<(i32, Value), ApiError> {
        let token = req.bearer.as_deref().ok_or_else(ApiError::unauthorized)?;
        let uid = repository::sessions()
            .lock()
            .ok()
            .and_then(|s| s.user_id(token))
            .ok_or_else(ApiError::unauthorized)?;
        let r = repository::user_by_id(&self.context.pool, uid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(ApiError::unauthorized)?;
        Ok((
            uid,
            json!({"id":r.id,"username":r.username,"email":r.email,"name":r.name,"role":r.role,"wallet_balance":r.wallet_balance.unwrap_or(0.0)}),
        ))
    }
    pub(crate) async fn admin(&self, req: &ApiRequest) -> Result<(i32, Value), ApiError> {
        let a = self.actor(req).await?;
        if a.1.get("role").and_then(Value::as_str) != Some("admin") {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden"));
        }
        Ok(a)
    }
    pub(crate) async fn order_json(&self, oid: i32) -> Result<Value, ApiError> {
        let o = repository::orders::detail(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let rows = repository::orders::items(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let items = rows
            .into_iter()
            .map(|r| {
                json!({
                    "product_id": r.product_id,
                    "sku": r.sku,
                    "name": r.name,
                    "quantity": r.quantity,
                    "price": r.price
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({
            "id": o.id,
            "order_no": o.order_no,
            "user_id": o.user_id,
            "status": o.status,
            "total": o.total,
            "paid_amount": o.paid_amount,
            "refunded": o.refunded,
            "coupon": o.coupon,
            "created_at": legacy_datetime(o.created_at),
            "items": items
        }))
    }
}
