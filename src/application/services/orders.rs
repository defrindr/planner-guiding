use super::{
    created, err, ok,
    shared::{id, money},
    ShopCore,
};
use crate::domain::calculate_total;
use crate::http::{ApiError, ApiRequest, ApiResponse};
use crate::repository::{coupons, orders, products, wallet};
use axum::http::StatusCode;
use chrono::Utc;
use serde_json::Value;

impl ShopCore {
    pub(crate) async fn create_order(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let items = req
            .body
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if items.is_empty() {
            return err(StatusCode::BAD_REQUEST, "items required");
        }
        let mut tx = self
            .context
            .pool
            .begin()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let no = orders::next_order_no(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let code = req.body.get("coupon").and_then(Value::as_str);
        let pct = if let Some(c) = code {
            let r = coupons::by_code(&mut *tx, c)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
                .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "invalid coupon"))?;
            if r.expires < Utc::now().date_naive().to_string() {
                return err(StatusCode::BAD_REQUEST, "coupon expired");
            }
            if r.single_use && r.used_count > 0 {
                return err(StatusCode::CONFLICT, "coupon already used");
            }
            r.percent
        } else {
            0
        };
        let notify_url = req.body.get("notify_url").and_then(Value::as_str);
        let oid = orders::insert(&mut *tx, no, uid, code, notify_url)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let mut sub = 0.0;
        for it in items {
            let pid = it.get("product_id").and_then(Value::as_i64).unwrap_or(0) as i32;
            let qty = it.get("quantity").and_then(Value::as_i64).unwrap_or(1) as i32;
            let p = products::for_order(&mut *tx, pid)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
                .ok_or_else(|| {
                    ApiError::new(StatusCode::NOT_FOUND, format!("product {} not found", pid))
                })?;
            if p.stock < qty {
                return err(
                    StatusCode::CONFLICT,
                    format!("insufficient stock for {}", p.sku),
                );
            }
            let price = p.price;
            orders::insert_item(&mut *tx, oid, pid, qty, price)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            products::decrement_stock(&mut *tx, pid, qty)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            sub += price * qty as f64;
        }
        let total = calculate_total(sub, pct);
        orders::set_total(&mut *tx, oid, total)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        if let Some(c) = code {
            coupons::increment_usage(&mut *tx, c)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        }
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        self.notify_order(notify_url, oid, no, "pending");
        created(self.order_json(oid).await?)
    }

    pub(crate) async fn cancel(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let _ = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = orders::for_cancel(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.status == "cancelled" {
            return err(StatusCode::CONFLICT, "already cancelled");
        }
        let mut tx = self
            .context
            .pool
            .begin()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        orders::restock_items(&mut *tx, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        if ["paid", "shipped"].contains(&o.status.as_str()) {
            wallet::credit(&mut *tx, o.user_id, o.total)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        }
        orders::mark_cancelled(&mut *tx, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(self.order_json(oid).await?)
    }

    pub(crate) async fn refund(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let _ = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = orders::for_refund(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.status == "pending" {
            return err(StatusCode::CONFLICT, "order not paid");
        }
        let a = req
            .body
            .get("amount")
            .map(|_| money(req.body.get("amount")))
            .unwrap_or(o.total);
        wallet::credit(&self.context.pool, o.user_id, a)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        orders::apply_refund(&self.context.pool, oid, a)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(self.order_json(oid).await?)
    }
}
