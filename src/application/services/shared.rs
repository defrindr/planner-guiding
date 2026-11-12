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
    pub(crate) fn notify_order(
        &self,
        notify_url: Option<&str>,
        order_id: i32,
        order_no: i32,
        status: &str,
    ) {
        let Some(url) = notify_url.filter(|url| !url.is_empty()) else {
            return;
        };
        let client = self.context.http_client.clone();
        let url = url.to_owned();
        let status = status.to_owned();
        tokio::spawn(async move {
            let _ = client
                .post(url)
                .json(&json!({
                    "order_id": order_id,
                    "order_no": order_no,
                    "status": status
                }))
                .send()
                .await;
        });
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::products::ProductRecord;
    use serde_json::json;

    #[test]
    fn path_id_parses_numbers_and_rejects_malformed_values() {
        assert_eq!(id("/api/products/7", "/api/products/"), Some(7));
        assert_eq!(id("/api/products/-1", "/api/products/"), Some(-1));
        assert_eq!(id("/api/orders/9/pay", "/api/orders/"), Some(9));
        assert_eq!(id("/api/products/abc", "/api/products/"), None);
        assert_eq!(id("/api/orders/7", "/api/products/"), None);
        assert_eq!(id("/api/products/", "/api/products/"), None);
    }

    #[test]
    fn int_query_parser_uses_default_for_invalid_values() {
        let mut query = BTreeMap::new();
        assert_eq!(int(&query, "page", 1), 1);
        query.insert("page".into(), "5".into());
        assert_eq!(int(&query, "page", 1), 5);
        query.insert("page".into(), "-2".into());
        assert_eq!(int(&query, "page", 1), -2);
        query.insert("page".into(), "abc".into());
        assert_eq!(int(&query, "page", 1), 1);
        query.insert("page".into(), "999999999999999999999".into());
        assert_eq!(int(&query, "page", 1), 1);
    }

    #[test]
    fn money_accepts_json_numbers_only() {
        assert_eq!(money(Some(&json!(12.5))), 12.5);
        assert_eq!(money(Some(&json!(3))), 3.0);
        assert_eq!(money(Some(&json!("12.5"))), 0.0);
        assert_eq!(money(Some(&Value::Null)), 0.0);
        assert_eq!(money(None), 0.0);
    }

    #[test]
    fn text_accepts_json_strings_only() {
        assert_eq!(text(Some(&json!("hello"))), "hello");
        assert_eq!(text(Some(&json!(1))), "");
        assert_eq!(text(Some(&Value::Null)), "");
        assert_eq!(text(None), "");
    }

    #[test]
    fn legacy_datetime_formats_rfc1123_with_gmt() {
        let value = chrono::NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap();
        assert_eq!(
            legacy_datetime(Some(value)),
            Some("Thu, 12 Nov 2026 01:02:03 GMT".into())
        );
        assert_eq!(legacy_datetime(None), None);
    }

    #[test]
    fn report_date_accepts_valid_dates_and_rejects_invalid_ones() {
        assert!(report_date("2026-11-12").is_ok());
        assert!(report_date("2024-02-29").is_ok());
        assert!(report_date("2026-02-29").is_err());
        assert!(report_date("not-a-date").is_err());
        assert!(report_date("").is_err());
    }

    #[test]
    fn product_json_preserves_public_contract_and_null_fallbacks() {
        let created_at = chrono::NaiveDate::from_ymd_opt(2026, 11, 12)
            .unwrap()
            .and_hms_opt(1, 2, 3)
            .unwrap();
        let value = product_json(&ProductRecord {
            id: 7,
            sku: Some("SKU-7".into()),
            name: Some("Widget".into()),
            price: Some(12.5),
            stock: Some(3),
            created_at: Some(created_at),
        });
        assert_eq!(value["id"], 7);
        assert_eq!(value["sku"], "SKU-7");
        assert_eq!(value["price"], 12.5);
        assert_eq!(value["stock"], 3);
        assert_eq!(value["created_at"], "Thu, 12 Nov 2026 01:02:03 GMT");

        let nulls = product_json(&ProductRecord {
            id: 8,
            sku: None,
            name: None,
            price: None,
            stock: None,
            created_at: None,
        });
        assert_eq!(nulls["sku"], Value::Null);
        assert_eq!(nulls["name"], Value::Null);
        assert_eq!(nulls["price"], 0.0);
        assert_eq!(nulls["stock"], 0);
        assert_eq!(nulls["created_at"], Value::Null);
    }
}
