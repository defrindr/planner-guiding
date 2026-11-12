use super::{err, ok, shared::money, ShopCore};
use crate::http::{ApiError, ApiRequest, ApiResponse};
use crate::repository::wallet;
use axum::http::StatusCode;
use serde_json::{json, Value};

impl ShopCore {
    pub(crate) async fn transfer(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let to = req
            .body
            .get("to_user_id")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
        let a = money(req.body.get("amount"));
        let mut tx = self
            .context
            .pool
            .begin()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let s = wallet::for_update_one(&mut *tx, uid)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let d = wallet::for_update(&mut *tx, to)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "recipient not found"))?;
        if s.wallet_balance.unwrap_or(0.0) < a {
            return err(StatusCode::PAYMENT_REQUIRED, "insufficient balance");
        }
        wallet::debit(&mut *tx, uid, a)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        wallet::credit(&mut *tx, d.id, a)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(json!({"balance":s.wallet_balance.unwrap_or(0.0)-a}))
    }
}
