use super::{
    created, err, ok,
    shared::{id, int, legacy_datetime, money, product_json, text},
    ShopCore,
};
use crate::{
    domain::line_total,
    http::{ApiError, ApiRequest, ApiResponse, HttpMethod},
    repository,
};
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use serde_json::{json, Value};
use std::{fs, path::Path};

impl ShopCore {
    pub(crate) async fn dispatch_inner(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let p = req.path.as_str();
        match (req.method, p) {
            (HttpMethod::Post, "/api/register") => {
                let u = text(req.body.get("username"));
                let e = text(req.body.get("email"));
                let pw = text(req.body.get("password"));
                if u.is_empty() || e.is_empty() || pw.is_empty() {
                    return err(
                        StatusCode::BAD_REQUEST,
                        "username, email and password are required",
                    );
                }
                if repository::users::user_by_username_or_email(&self.context.pool, &u, &e)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                    .is_some()
                {
                    return err(StatusCode::CONFLICT, "username taken");
                }
                let n = req.body.get("name").and_then(Value::as_str).unwrap_or(&u);
                let uid = repository::insert_user(&self.context.pool, &u, &e, n, &pw)
                    .await
                    .map_err(|_| ApiError::new(StatusCode::CONFLICT, "username taken"))?;
                created(json!({"id":uid,"username":u}))
            }
            (HttpMethod::Post, "/api/login") => {
                let u = text(req.body.get("username"));
                let pw = text(req.body.get("password"));
                let Some(r) = repository::authenticate_user(&self.context.pool, &u, &pw)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                else {
                    return err(StatusCode::UNAUTHORIZED, "invalid credentials");
                };
                let token = crate::domain::make_token(
                    r.username.as_deref().unwrap_or(&u),
                    Utc::now().timestamp(),
                );
                if let Ok(mut s) = repository::sessions().lock() {
                    s.insert(token.clone(), r.id);
                }
                ok(json!({"token":token}))
            }
            (HttpMethod::Post, "/api/logout") => {
                let _ = self.actor(&req).await?;
                ok(json!({"ok":true}))
            }
            (HttpMethod::Get, "/api/users/me") => {
                let (_, u) = self.actor(&req).await?;
                ok(u)
            }
            (HttpMethod::Patch, "/api/users/me") => {
                let (uid, _) = self.actor(&req).await?;
                for (key, field) in [
                    ("username", repository::users::ProfileField::Username),
                    ("email", repository::users::ProfileField::Email),
                    ("name", repository::users::ProfileField::Name),
                ] {
                    if let Some(v) = req.body.get(key).and_then(Value::as_str) {
                        repository::users::update_profile_field(&self.context.pool, uid, field, v)
                            .await
                            .map_err(|_| {
                                ApiError::new(StatusCode::BAD_REQUEST, "invalid update")
                            })?;
                    }
                }

                let (_, u) = self.actor(&req).await?;
                ok(u)
            }
            (HttpMethod::Get, "/api/products") => self.products(&req).await,
            (HttpMethod::Get, _) if p.starts_with("/api/products/") => {
                let pid = id(p, "/api/products/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let Some(product) = repository::products::by_id(&self.context.pool, pid)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                else {
                    return err(StatusCode::NOT_FOUND, "not found");
                };
                ok(product_json(&product))
            }
            (HttpMethod::Post, "/api/products") => {
                let _ = self.admin(&req).await?;
                let sku = text(req.body.get("sku"));
                let name = text(req.body.get("name"));
                let price = money(req.body.get("price"));
                let stock = req.body.get("stock").and_then(Value::as_i64).unwrap_or(0) as i32;
                let product =
                    repository::products::insert(&self.context.pool, &sku, &name, price, stock)
                        .await
                        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid product"))?;
                created(product_json(&product))
            }
            (HttpMethod::Put, _) if p.starts_with("/api/products/") => {
                let _ = self.admin(&req).await?;
                let pid = id(p, "/api/products/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let Some(old) = repository::products::by_id(&self.context.pool, pid)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                else {
                    return err(StatusCode::NOT_FOUND, "not found");
                };
                let old_sku = old.sku.clone().unwrap_or_default();
                let old_name = old.name.clone().unwrap_or_default();
                let sku = req
                    .body
                    .get("sku")
                    .and_then(Value::as_str)
                    .unwrap_or(&old_sku);
                let name = req
                    .body
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(&old_name);
                let price = req
                    .body
                    .get("price")
                    .map(|_| money(req.body.get("price")))
                    .unwrap_or(old.price.unwrap_or(0.0));
                let stock = req
                    .body
                    .get("stock")
                    .and_then(Value::as_i64)
                    .unwrap_or(old.stock.unwrap_or(0) as i64) as i32;
                let product =
                    repository::products::update(&self.context.pool, pid, sku, name, price, stock)
                        .await
                        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid product"))?
                        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(product_json(&product))
            }
            (HttpMethod::Post, "/api/orders") => self.create_order(req).await,
            (HttpMethod::Get, "/api/orders") => {
                let (uid, _) = self.actor(&req).await?;
                let page = int(&req.query, "page", 1);
                let lim = int(&req.query, "limit", 20);
                let rows = repository::orders::list_ids_for_user(
                    &self.context.pool,
                    uid,
                    lim,
                    (page - 1) * lim,
                )
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
                let mut out = Vec::new();
                for oid in rows {
                    out.push(self.order_json(oid).await?);
                }
                ok(json!(out))
            }
            (HttpMethod::Get, _) if p.starts_with("/api/orders/") && !p.contains("/invoice") => {
                let _ = self.actor(&req).await?;
                let oid = id(p, "/api/orders/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(self.order_json(oid).await?)
            }
            (HttpMethod::Get, _) if p.ends_with("/invoice") => {
                let oid = p
                    .strip_prefix("/api/orders/")
                    .and_then(|x| x.strip_suffix("/invoice"))
                    .and_then(|x| x.parse().ok())
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let o = self.order_json(oid).await?;
                let uid = o["user_id"].as_i64().unwrap_or(0) as i32;
                let _ = self.actor(&req).await?;
                let u = repository::users::name_email(&self.context.pool, uid)
                    .await
                    .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not found"))?
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let lines=o["items"].as_array().unwrap().iter().map(|x|json!({"sku":x["sku"],"name":x["name"],"quantity":x["quantity"],"price":x["price"],"line_total":line_total(x["price"].as_f64().unwrap_or(0.0),x["quantity"].as_i64().unwrap_or(0) as i32)})).collect::<Vec<_>>();
                ok(
                    json!({"order_no":o["order_no"],"customer":{"name":u.name,"email":u.email},"lines":lines,"total":o["total"],"status":o["status"]}),
                )
            }
            (HttpMethod::Post, _) if p.ends_with("/pay") => self.pay(req).await,
            (HttpMethod::Post, _) if p.ends_with("/cancel") => self.cancel(req).await,
            (HttpMethod::Post, _) if p.ends_with("/refund") => self.refund(req).await,
            (HttpMethod::Get, "/api/wallet") => {
                let (_, u) = self.actor(&req).await?;
                ok(json!({"balance":u["wallet_balance"]}))
            }
            (HttpMethod::Post, "/api/wallet/topup") => {
                let (uid, _) = self.actor(&req).await?;
                let a = money(req.body.get("amount"));
                let balance = repository::wallet::topup(&self.context.pool, uid, a)
                    .await
                    .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid amount"))?;
                ok(json!({"balance":balance}))
            }
            (HttpMethod::Post, "/api/wallet/transfer") => self.transfer(req).await,
            (HttpMethod::Get, "/api/reports/sales") => self.report(req).await,
            (HttpMethod::Post, "/api/reports/export") => {
                let _ = self.admin(&req).await?;
                let f = self
                    .export(
                        &text(req.body.get("filename")),
                        req.body
                            .get("from")
                            .and_then(Value::as_str)
                            .unwrap_or("1970-01-01"),
                        req.body
                            .get("to")
                            .and_then(Value::as_str)
                            .unwrap_or("2999-12-31"),
                    )
                    .await?;
                ok(json!({"file":f}))
            }
            (HttpMethod::Get, "/api/reports/download") => {
                let _ = self.admin(&req).await?;
                let n = req
                    .query
                    .get("file")
                    .and_then(|x| Path::new(x).file_name())
                    .and_then(|x| x.to_str())
                    .unwrap_or("");
                let p = self.context.config.export_dir.join(n);
                if !p.exists() {
                    return err(StatusCode::NOT_FOUND, "not found");
                }
                let data =
                    fs::read(p).map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let mut h = HeaderMap::new();
                h.insert("content-type", "application/gzip".parse().unwrap());
                Ok(ApiResponse {
                    status: StatusCode::OK,
                    body: Value::Null,
                    headers: h,
                    raw_body: Some(data),
                })
            }
            (HttpMethod::Get, "/api/admin/users") => {
                let _ = self.admin(&req).await?;
                let rows = repository::users::list_users(&self.context.pool)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?;
                ok(json!(rows
                    .into_iter()
                    .map(|r| json!({
                        "id": r.id,
                        "username": r.username,
                        "email": r.email,
                        "name": r.name,
                        "role": r.role,
                        "wallet_balance": r.wallet_balance.unwrap_or(0.0),
                        "password_hash": r.password_hash,
                        "created_at": legacy_datetime(r.created_at)
                    }))
                    .collect::<Vec<_>>()))
            }
            (HttpMethod::Post, _) if p.starts_with("/api/admin/users/") => {
                let _ = self.admin(&req).await?;
                let uid = id(p, "/api/admin/users/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let role = text(req.body.get("role"));
                let n = repository::users::update_role(&self.context.pool, uid, &role)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(json!({"id":n.id,"username":n.username,"role":n.role}))
            }
            (HttpMethod::Post, _) if p.starts_with("/api/admin/orders/") => {
                let _ = self.admin(&req).await?;
                let oid = id(p, "/api/admin/orders/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let st = text(req.body.get("status"));
                let n = repository::orders::update_status(&self.context.pool, oid, &st)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(json!({"id":n.id,"order_no":n.order_no,"status":n.status}))
            }
            (HttpMethod::Post, "/api/webhooks/bank") => {
                let b = &req.body;
                let oid = b.get("order_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                if b.get("status").and_then(Value::as_str) == Some("settled") {
                    repository::payments::settle(&self.context.pool, oid, &text(b.get("bank_ref")))
                        .await
                        .map_err(|_| {
                            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                        })?;
                    repository::orders::mark_paid_if_pending(&self.context.pool, oid)
                        .await
                        .map_err(|_| {
                            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                        })?;
                }
                ok(json!({"ok":true}))
            }
            _ => err(StatusCode::NOT_FOUND, "not found"),
        }
    }
}
