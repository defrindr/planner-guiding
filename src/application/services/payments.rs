use super::{err, ok, shared::id, ShopCore};
use crate::http::{ApiError, ApiRequest, ApiResponse};
use crate::repository::{orders, payments, wallet};
use axum::http::StatusCode;
use serde_json::{json, Value};

impl ShopCore {
    pub(crate) async fn bank_pay(&self, oid: i32, amount: f64) -> Result<Option<String>, ApiError> {
        for _ in 0..3 {
            if let Ok(r) = self
                .context
                .http_client
                .post(format!("{}/charge", self.context.config.bank_url))
                .json(&json!({"order_id":oid,"amount":amount}))
                .send()
                .await
            {
                if r.status().is_success() {
                    if let Ok(v) = r.json::<Value>().await {
                        return Ok(v.get("bank_ref").and_then(Value::as_str).map(str::to_owned));
                    }
                }
            }
        }
        Ok(None)
    }

    pub(crate) async fn pay(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = orders::for_payment(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.user_id != uid {
            return err(StatusCode::NOT_FOUND, "not found");
        }
        if o.status != "pending" {
            return err(StatusCode::CONFLICT, "order not payable");
        }
        let method = req
            .body
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("wallet");
        if method == "wallet" {
            let mut tx =
                self.context.pool.begin().await.map_err(|_| {
                    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                })?;
            let bal = wallet::balance_for_update(&mut *tx, uid)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            if bal < o.total {
                return err(StatusCode::PAYMENT_REQUIRED, "insufficient wallet balance");
            }
            wallet::debit(&mut *tx, uid, o.total)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            orders::mark_paid(&mut *tx, oid)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            tx.commit()
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            self.notify_order(o.notify_url.as_deref(), oid, o.order_no, "paid");
            return ok(json!({
                "order_id": oid,
                "status": "paid",
                "method": "wallet",
                "bank_ref": Value::Null
            }));
        }
        if method != "bank" {
            return err(StatusCode::BAD_REQUEST, "unknown method");
        }
        let Some(r) = self.bank_pay(oid, o.total).await? else {
            return err(StatusCode::BAD_GATEWAY, "payment failed");
        };
        payments::insert_captured(&self.context.pool, oid, o.total, &r)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        orders::mark_paid(&self.context.pool, oid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        self.notify_order(o.notify_url.as_deref(), oid, o.order_no, "paid");
        ok(json!({"order_id":oid,"status":"paid","method":"bank","bank_ref":r}))
    }
}
