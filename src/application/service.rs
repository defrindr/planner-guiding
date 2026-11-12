use std::{collections::BTreeMap, fs, io::Write, path::Path};

use anyhow::Result;
use axum::http::{HeaderMap, StatusCode};
use chrono::Utc;
use flate2::{write::GzEncoder, Compression};
use serde_json::{json, Value};
use sqlx::Row;

use crate::{
    application::ApplicationContext,
    config::Config,
    domain::{calculate_total, line_total},
    http::{ApiError, ApiFuture, ApiRequest, ApiResponse, HttpCore, HttpMethod},
    repository,
};

#[derive(Clone)]
pub struct ShopCore {
    pub context: ApplicationContext,
}

impl ShopCore {
    pub fn new(pool: sqlx::PgPool, config: Config) -> Self {
        Self {
            context: ApplicationContext::new(pool, config),
        }
    }
}

fn ok(body: Value) -> Result<ApiResponse, ApiError> {
    Ok(ApiResponse {
        status: StatusCode::OK,
        body,
        headers: HeaderMap::new(),
        raw_body: None,
    })
}
fn created(body: Value) -> Result<ApiResponse, ApiError> {
    Ok(ApiResponse {
        status: StatusCode::CREATED,
        body,
        headers: HeaderMap::new(),
        raw_body: None,
    })
}
fn err(status: StatusCode, msg: impl Into<String>) -> Result<ApiResponse, ApiError> {
    Err(ApiError::new(status, msg))
}
fn id(path: &str, marker: &str) -> Option<i32> {
    path.strip_prefix(marker)?.split('/').next()?.parse().ok()
}
fn int(q: &BTreeMap<String, String>, k: &str, d: i64) -> i64 {
    q.get(k).and_then(|x| x.parse().ok()).unwrap_or(d)
}
fn money(v: Option<&Value>) -> f64 {
    v.and_then(Value::as_f64)
        .or_else(|| v.and_then(Value::as_i64).map(|x| x as f64))
        .unwrap_or(0.0)
}
fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or("").to_string()
}

fn legacy_datetime(value: Option<chrono::NaiveDateTime>) -> Option<String> {
    value.map(|value| format!("{} GMT", value.format("%a, %d %b %Y %H:%M:%S")))
}

fn report_date(value: &str) -> Result<chrono::NaiveDate, ApiError> {
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid date"))
}

impl ShopCore {
    async fn actor(&self, req: &ApiRequest) -> Result<(i32, Value), ApiError> {
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
    async fn admin(&self, req: &ApiRequest) -> Result<(i32, Value), ApiError> {
        let a = self.actor(req).await?;
        if a.1.get("role").and_then(Value::as_str) != Some("admin") {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "forbidden"));
        }
        Ok(a)
    }
    async fn order_json(&self, oid: i32) -> Result<Value, ApiError> {
        let o=sqlx::query("SELECT id,order_no,user_id,status,total,paid_amount,refunded,coupon,created_at FROM orders WHERE id=$1").bind(oid).fetch_optional(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?.ok_or_else(||ApiError::new(StatusCode::NOT_FOUND,"not found"))?;
        let rows=sqlx::query("SELECT i.product_id,i.quantity,i.price,p.sku,p.name FROM order_items i LEFT JOIN products p ON p.id=i.product_id WHERE i.order_id=$1 ORDER BY i.id").bind(oid).fetch_all(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
        let items=rows.into_iter().map(|r|json!({"product_id":r.get::<i32,_>("product_id"),"sku":r.get::<Option<String>,_>("sku"),"name":r.get::<Option<String>,_>("name"),"quantity":r.get::<i32,_>("quantity"),"price":r.get::<f64,_>("price")})).collect::<Vec<_>>();
        Ok(
            json!({"id":o.get::<i32,_>("id"),"order_no":o.get::<i32,_>("order_no"),"user_id":o.get::<i32,_>("user_id"),"status":o.get::<String,_>("status"),"total":o.get::<f64,_>("total"),"paid_amount":o.get::<f64,_>("paid_amount"),"refunded":o.get::<f64,_>("refunded"),"coupon":o.get::<Option<String>,_>("coupon"),"created_at":legacy_datetime(o.get::<Option<chrono::NaiveDateTime>,_>("created_at")),"items":items}),
        )
    }
    async fn products(&self, req: &ApiRequest) -> Result<ApiResponse, ApiError> {
        let q = req.query.get("q").cloned();
        let sort = match req.query.get("sort").map(String::as_str) {
            Some("price") => "price",
            Some("name") => "name",
            Some("stock") => "stock",
            _ => "id",
        };
        let page = int(&req.query, "page", 1).max(1);
        let limit = int(&req.query, "limit", 20);
        let mut sql = format!(
            "SELECT id,sku,name,price,stock,created_at FROM products {} ORDER BY {}",
            if q.is_some() {
                "WHERE name ILIKE $1"
            } else {
                ""
            },
            sort
        );
        if limit >= 0 {
            sql.push_str(&format!(" LIMIT {} OFFSET {}", limit, (page - 1) * limit));
        }
        let mut x = sqlx::query(&sql);
        if let Some(q) = q {
            x = x.bind(format!("%{}%", q));
        }
        let rows = x
            .fetch_all(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(Value::Array(rows.into_iter().map(|r|json!({"id":r.get::<i32,_>("id"),"sku":r.get::<Option<String>,_>("sku"),"name":r.get::<Option<String>,_>("name"),"price":r.get::<Option<f64>,_>("price").unwrap_or(0.0),"stock":r.get::<Option<i32>,_>("stock").unwrap_or(0),"created_at":legacy_datetime(r.get::<Option<chrono::NaiveDateTime>,_>("created_at"))})).collect()))
    }
    async fn bank_pay(&self, oid: i32, amount: f64) -> Result<Option<String>, ApiError> {
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
    async fn export(&self, filename: &str, from: &str, to: &str) -> Result<String, ApiError> {
        let from_date = report_date(from)?;
        let to_date = report_date(to)?;
        let safe = Path::new(filename)
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("sales.csv");
        fs::create_dir_all(&self.context.config.export_dir)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        let rows=sqlx::query("SELECT o.order_no,o.created_at,o.status,u.username,p.name AS product,i.quantity,i.price FROM orders o JOIN users u ON u.id=o.user_id JOIN order_items i ON i.order_id=o.id JOIN products p ON p.id=i.product_id WHERE o.created_at >= $1 AND o.created_at < $2 ORDER BY o.id").bind(from_date).bind(to_date).fetch_all(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
        let mut s = "order_no,date,status,user,product,qty,price\n".to_string();
        for r in rows {
            s.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                r.get::<i32, _>("order_no"),
                r.get::<chrono::NaiveDateTime, _>("created_at"),
                r.get::<String, _>("status"),
                r.get::<String, _>("username"),
                r.get::<String, _>("product"),
                r.get::<i32, _>("quantity"),
                r.get::<f64, _>("price")
            ));
        }
        let p = self.context.config.export_dir.join(safe);
        fs::write(&p, &s)
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        let gz = Path::new(&format!("{}.gz", p.display())).to_path_buf();
        let mut z = GzEncoder::new(Vec::new(), Compression::default());
        z.write_all(s.as_bytes())
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        fs::write(
            &gz,
            z.finish()
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?,
        )
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "export error"))?;
        Ok(gz.file_name().unwrap().to_string_lossy().into())
    }
}

impl HttpCore for ShopCore {
    fn dispatch(&self, req: ApiRequest) -> ApiFuture {
        let s = self.clone();
        Box::pin(async move { s.dispatch_inner(req).await })
    }
}

impl ShopCore {
    async fn dispatch_inner(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
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
                if sqlx::query("SELECT id FROM users WHERE username=$1 OR email=$2")
                    .bind(&u)
                    .bind(&e)
                    .fetch_optional(&self.context.pool)
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
                for k in ["username", "email", "name"] {
                    if let Some(v) = req.body.get(k).and_then(Value::as_str) {
                        let sql = format!("UPDATE users SET {}=$1 WHERE id=$2", k);
                        sqlx::query(&sql)
                            .bind(v)
                            .bind(uid)
                            .execute(&self.context.pool)
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
                let Some(r) = sqlx::query(
                    "SELECT id,sku,name,price,stock,created_at FROM products WHERE id=$1",
                )
                .bind(pid)
                .fetch_optional(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
                else {
                    return err(StatusCode::NOT_FOUND, "not found");
                };
                ok(
                    json!({"id":r.get::<i32,_>("id"),"sku":r.get::<Option<String>,_>("sku"),"name":r.get::<Option<String>,_>("name"),"price":r.get::<Option<f64>,_>("price").unwrap_or(0.0),"stock":r.get::<Option<i32>,_>("stock").unwrap_or(0),"created_at":legacy_datetime(r.get::<Option<chrono::NaiveDateTime>,_>("created_at"))}),
                )
            }
            (HttpMethod::Post, "/api/products") => {
                let _ = self.admin(&req).await?;
                let r = sqlx::query_scalar::<_, i32>(
                    "INSERT INTO products(sku,name,price,stock) VALUES($1,$2,$3,$4) RETURNING id",
                )
                .bind(text(req.body.get("sku")))
                .bind(text(req.body.get("name")))
                .bind(money(req.body.get("price")))
                .bind(req.body.get("stock").and_then(Value::as_i64).unwrap_or(0) as i32)
                .fetch_one(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid product"))?;
                let p = sqlx::query(
                    "SELECT id,sku,name,price,stock,created_at FROM products WHERE id=$1",
                )
                .bind(r)
                .fetch_one(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
                created(
                    json!({"id":p.get::<i32,_>("id"),"sku":p.get::<Option<String>,_>("sku"),"name":p.get::<Option<String>,_>("name"),"price":p.get::<Option<f64>,_>("price").unwrap_or(0.0),"stock":p.get::<Option<i32>,_>("stock").unwrap_or(0),"created_at":legacy_datetime(p.get::<Option<chrono::NaiveDateTime>,_>("created_at"))}),
                )
            }
            (HttpMethod::Put, _) if p.starts_with("/api/products/") => {
                let _ = self.admin(&req).await?;
                let pid = id(p, "/api/products/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let Some(old) =
                    sqlx::query("SELECT sku,name,price,stock FROM products WHERE id=$1")
                        .bind(pid)
                        .fetch_optional(&self.context.pool)
                        .await
                        .map_err(|_| {
                            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                        })?
                else {
                    return err(StatusCode::NOT_FOUND, "not found");
                };
                sqlx::query("UPDATE products SET sku=$1,name=$2,price=$3,stock=$4 WHERE id=$5")
                    .bind(
                        req.body.get("sku").and_then(Value::as_str).unwrap_or(
                            old.get::<Option<String>, _>("sku").as_deref().unwrap_or(""),
                        ),
                    )
                    .bind(
                        req.body.get("name").and_then(Value::as_str).unwrap_or(
                            old.get::<Option<String>, _>("name")
                                .as_deref()
                                .unwrap_or(""),
                        ),
                    )
                    .bind(
                        req.body
                            .get("price")
                            .map(|_| money(req.body.get("price")))
                            .unwrap_or(old.get::<Option<f64>, _>("price").unwrap_or(0.0)),
                    )
                    .bind(
                        req.body
                            .get("stock")
                            .and_then(Value::as_i64)
                            .unwrap_or(old.get::<Option<i32>, _>("stock").unwrap_or(0) as i64)
                            as i32,
                    )
                    .bind(pid)
                    .execute(&self.context.pool)
                    .await
                    .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid product"))?;
                let p = sqlx::query(
                    "SELECT id,sku,name,price,stock,created_at FROM products WHERE id=$1",
                )
                .bind(pid)
                .fetch_one(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
                ok(
                    json!({"id":p.get::<i32,_>("id"),"sku":p.get::<Option<String>,_>("sku"),"name":p.get::<Option<String>,_>("name"),"price":p.get::<Option<f64>,_>("price").unwrap_or(0.0),"stock":p.get::<Option<i32>,_>("stock").unwrap_or(0),"created_at":legacy_datetime(p.get::<Option<chrono::NaiveDateTime>,_>("created_at"))}),
                )
            }
            (HttpMethod::Post, "/api/orders") => self.create_order(req).await,
            (HttpMethod::Get, "/api/orders") => {
                let (uid, _) = self.actor(&req).await?;
                let page = int(&req.query, "page", 1);
                let lim = int(&req.query, "limit", 20);
                let rows = sqlx::query(
                    "SELECT id FROM orders WHERE user_id=$1 ORDER BY id DESC LIMIT $2 OFFSET $3",
                )
                .bind(uid)
                .bind(lim)
                .bind((page - 1) * lim)
                .fetch_all(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(self.order_json(r.get("id")).await?);
                }
                ok(json!(out))
            }
            (HttpMethod::Get, _) if p.starts_with("/api/orders/") && !p.contains("/invoice") => {
                let (uid, _) = self.actor(&req).await?;
                let oid = id(p, "/api/orders/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let v = self.order_json(oid).await?;
                if v.get("user_id").and_then(Value::as_i64) != Some(uid as i64) {
                    return err(StatusCode::NOT_FOUND, "not found");
                }
                ok(v)
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
                let u = sqlx::query("SELECT name,email FROM users WHERE id=$1")
                    .bind(uid)
                    .fetch_one(&self.context.pool)
                    .await
                    .map_err(|_| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let lines=o["items"].as_array().unwrap().iter().map(|x|json!({"sku":x["sku"],"name":x["name"],"quantity":x["quantity"],"price":x["price"],"line_total":line_total(x["price"].as_f64().unwrap_or(0.0),x["quantity"].as_i64().unwrap_or(0) as i32)})).collect::<Vec<_>>();
                ok(
                    json!({"order_no":o["order_no"],"customer":{"name":u.get::<Option<String>,_>("name"),"email":u.get::<Option<String>,_>("email")},"lines":lines,"total":o["total"],"status":o["status"]}),
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
                let r=sqlx::query_scalar::<_,f64>("UPDATE users SET wallet_balance=COALESCE(wallet_balance,0)+$1 WHERE id=$2 RETURNING wallet_balance").bind(a).bind(uid).fetch_one(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::BAD_REQUEST,"invalid amount"))?;
                ok(json!({"balance":r}))
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
                let rows=sqlx::query("SELECT id,username,email,name,role,wallet_balance,password_hash,created_at FROM users ORDER BY id").fetch_all(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
                ok(json!(rows.into_iter().map(|r|json!({"id":r.get::<i32,_>("id"),"username":r.get::<Option<String>,_>("username"),"email":r.get::<Option<String>,_>("email"),"name":r.get::<Option<String>,_>("name"),"role":r.get::<Option<String>,_>("role"),"wallet_balance":r.get::<Option<f64>,_>("wallet_balance").unwrap_or(0.0),"password_hash":r.get::<Option<String>,_>("password_hash"),"created_at":legacy_datetime(r.get::<Option<chrono::NaiveDateTime>,_>("created_at"))})).collect::<Vec<_>>()))
            }
            (HttpMethod::Post, _) if p.starts_with("/api/admin/users/") => {
                let _ = self.admin(&req).await?;
                let uid = id(p, "/api/admin/users/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let role = text(req.body.get("role"));
                let n =
                    sqlx::query("UPDATE users SET role=$1 WHERE id=$2 RETURNING id,username,role")
                        .bind(if role.is_empty() { "user" } else { &role })
                        .bind(uid)
                        .fetch_optional(&self.context.pool)
                        .await
                        .map_err(|_| {
                            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                        })?
                        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(
                    json!({"id":n.get::<i32,_>("id"),"username":n.get::<Option<String>,_>("username"),"role":n.get::<Option<String>,_>("role")}),
                )
            }
            (HttpMethod::Post, _) if p.starts_with("/api/admin/orders/") => {
                let _ = self.admin(&req).await?;
                let oid = id(p, "/api/admin/orders/")
                    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                let st = text(req.body.get("status"));
                let n = sqlx::query(
                    "UPDATE orders SET status=$1 WHERE id=$2 RETURNING id,order_no,status",
                )
                .bind(st)
                .bind(oid)
                .fetch_optional(&self.context.pool)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
                .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
                ok(
                    json!({"id":n.get::<i32,_>("id"),"order_no":n.get::<i32,_>("order_no"),"status":n.get::<String,_>("status")}),
                )
            }
            (HttpMethod::Post, "/api/webhooks/bank") => {
                let b = &req.body;
                let oid = b.get("order_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                if b.get("status").and_then(Value::as_str) == Some("settled") {
                    sqlx::query(
                        "UPDATE payments SET status='settled' WHERE order_id=$1 AND bank_ref=$2",
                    )
                    .bind(oid)
                    .bind(text(b.get("bank_ref")))
                    .execute(&self.context.pool)
                    .await
                    .map_err(|_| {
                        ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                    })?;
                    sqlx::query("UPDATE orders SET status='paid',paid_amount=total WHERE id=$1 AND status='pending'").bind(oid).execute(&self.context.pool).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
                }
                ok(json!({"ok":true}))
            }
            _ => err(StatusCode::NOT_FOUND, "not found"),
        }
    }
    async fn create_order(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
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
        let no = sqlx::query_scalar::<_, i32>("SELECT COALESCE(MAX(order_no),1000)+1 FROM orders")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let code = req.body.get("coupon").and_then(Value::as_str);
        let pct = if let Some(c) = code {
            let r = sqlx::query(
                "SELECT percent,expires,single_use,used_count FROM coupons WHERE code=$1",
            )
            .bind(c)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "invalid coupon"))?;
            if r.get::<String, _>("expires") < Utc::now().date_naive().to_string() {
                return err(StatusCode::BAD_REQUEST, "coupon expired");
            }
            if r.get::<bool, _>("single_use") && r.get::<i32, _>("used_count") > 0 {
                return err(StatusCode::CONFLICT, "coupon already used");
            }
            r.get::<i32, _>("percent")
        } else {
            0
        };
        let oid=sqlx::query_scalar::<_,i32>("INSERT INTO orders(order_no,user_id,status,total,paid_amount,refunded,coupon,notify_url,created_at) VALUES($1,$2,'pending',0,0,0,$3,$4,now()) RETURNING id").bind(no).bind(uid).bind(code).bind(req.body.get("notify_url").and_then(Value::as_str)).fetch_one(&mut *tx).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
        let mut sub = 0.0;
        for it in items {
            let pid = it.get("product_id").and_then(Value::as_i64).unwrap_or(0) as i32;
            let qty = it.get("quantity").and_then(Value::as_i64).unwrap_or(1) as i32;
            let p = sqlx::query("SELECT sku,price,stock FROM products WHERE id=$1 FOR UPDATE")
                .bind(pid)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
                .ok_or_else(|| {
                    ApiError::new(StatusCode::NOT_FOUND, format!("product {} not found", pid))
                })?;
            if p.get::<i32, _>("stock") < qty {
                return err(
                    StatusCode::CONFLICT,
                    format!("insufficient stock for {}", p.get::<String, _>("sku")),
                );
            }
            let price = p.get::<f64, _>("price");
            sqlx::query(
                "INSERT INTO order_items(order_id,product_id,quantity,price) VALUES($1,$2,$3,$4)",
            )
            .bind(oid)
            .bind(pid)
            .bind(qty)
            .bind(price)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            sqlx::query("UPDATE products SET stock=stock-$1 WHERE id=$2")
                .bind(qty)
                .bind(pid)
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            sub += price * qty as f64;
        }
        let total = calculate_total(sub, pct);
        sqlx::query("UPDATE orders SET total=$1 WHERE id=$2")
            .bind(total)
            .bind(oid)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        if let Some(c) = code {
            sqlx::query("UPDATE coupons SET used_count=used_count+1 WHERE code=$1")
                .bind(c)
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        }
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        created(self.order_json(oid).await?)
    }
    async fn transfer(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
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
        let s = sqlx::query("SELECT wallet_balance FROM users WHERE id=$1 FOR UPDATE")
            .bind(uid)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        let d = sqlx::query("SELECT id,wallet_balance FROM users WHERE id=$1 FOR UPDATE")
            .bind(to)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "recipient not found"))?;
        if s.get::<Option<f64>, _>("wallet_balance").unwrap_or(0.0) < a {
            return err(StatusCode::PAYMENT_REQUIRED, "insufficient balance");
        }
        sqlx::query("UPDATE users SET wallet_balance=wallet_balance-$1 WHERE id=$2")
            .bind(a)
            .bind(uid)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        sqlx::query("UPDATE users SET wallet_balance=COALESCE(wallet_balance,0)+$1 WHERE id=$2")
            .bind(a)
            .bind(d.get::<i32, _>("id"))
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(json!({"balance":s.get::<Option<f64>,_>("wallet_balance").unwrap_or(0.0)-a}))
    }
    async fn pay(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = sqlx::query("SELECT user_id,status,total FROM orders WHERE id=$1")
            .bind(oid)
            .fetch_optional(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.get::<i32, _>("user_id") != uid {
            return err(StatusCode::NOT_FOUND, "not found");
        }
        if o.get::<String, _>("status") != "pending" {
            return err(StatusCode::CONFLICT, "order not payable");
        }
        let method = text(req.body.get("method"));
        if method == "wallet" {
            let mut tx =
                self.context.pool.begin().await.map_err(|_| {
                    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
                })?;
            let bal = sqlx::query_scalar::<_, f64>(
                "SELECT wallet_balance FROM users WHERE id=$1 FOR UPDATE",
            )
            .bind(uid)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            if bal < o.get("total") {
                return err(StatusCode::PAYMENT_REQUIRED, "insufficient wallet balance");
            }
            sqlx::query("UPDATE users SET wallet_balance=wallet_balance-$1 WHERE id=$2")
                .bind(o.get::<f64, _>("total"))
                .bind(uid)
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            sqlx::query("UPDATE orders SET status='paid',paid_amount=total WHERE id=$1")
                .bind(oid)
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            tx.commit()
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
            return ok(json!({
                "order_id": oid,
                "status": "paid",
                "method": "wallet",
                "bank_ref": Value::Null
            }));
        }
        let Some(r) = self.bank_pay(oid, o.get("total")).await? else {
            return err(StatusCode::BAD_GATEWAY, "payment failed");
        };
        sqlx::query("INSERT INTO payments(order_id,amount,method,bank_ref,status) VALUES($1,$2,'bank',$3,'captured')")
            .bind(oid)
            .bind(o.get::<f64, _>("total"))
            .bind(&r)
            .execute(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        sqlx::query("UPDATE orders SET status='paid', paid_amount=total WHERE id=$1")
            .bind(oid)
            .execute(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(json!({"order_id":oid,"status":"paid","method":"bank","bank_ref":r}))
    }
    async fn cancel(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = sqlx::query("SELECT user_id,status,total FROM orders WHERE id=$1")
            .bind(oid)
            .fetch_optional(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.get::<i32, _>("user_id") != uid {
            return err(StatusCode::NOT_FOUND, "not found");
        }
        if o.get::<String, _>("status") == "cancelled" {
            return err(StatusCode::CONFLICT, "already cancelled");
        }
        let mut tx = self
            .context
            .pool
            .begin()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        sqlx::query("UPDATE products p SET stock=p.stock+i.quantity FROM order_items i WHERE i.order_id=$1 AND i.product_id=p.id").bind(oid).execute(&mut *tx).await.map_err(|_|ApiError::new(StatusCode::INTERNAL_SERVER_ERROR,"database error"))?;
        if ["paid", "shipped"].contains(&o.get::<String, _>("status").as_str()) {
            sqlx::query("UPDATE users SET wallet_balance=wallet_balance+$1 WHERE id=$2")
                .bind(o.get::<f64, _>("total"))
                .bind(uid)
                .execute(&mut *tx)
                .await
                .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        }
        sqlx::query("UPDATE orders SET status='cancelled' WHERE id=$1")
            .bind(oid)
            .execute(&mut *tx)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        tx.commit()
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(self.order_json(oid).await?)
    }
    async fn refund(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let (uid, _) = self.actor(&req).await?;
        let oid = id(&req.path, "/api/orders/")
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        let o = sqlx::query("SELECT user_id,status,total,refunded FROM orders WHERE id=$1")
            .bind(oid)
            .fetch_optional(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?
            .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "not found"))?;
        if o.get::<i32, _>("user_id") != uid {
            return err(StatusCode::NOT_FOUND, "not found");
        }
        if o.get::<String, _>("status") == "pending" {
            return err(StatusCode::CONFLICT, "order not paid");
        }
        let a = req
            .body
            .get("amount")
            .map(|_| money(req.body.get("amount")))
            .unwrap_or(o.get::<f64, _>("total"));
        sqlx::query("UPDATE users SET wallet_balance=wallet_balance+$1 WHERE id=$2")
            .bind(a)
            .bind(uid)
            .execute(&self.context.pool)
            .await
            .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        sqlx::query(
            "UPDATE orders SET status='refunded',refunded=COALESCE(refunded,0)+$1 WHERE id=$2",
        )
        .bind(a)
        .bind(oid)
        .execute(&self.context.pool)
        .await
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error"))?;
        ok(self.order_json(oid).await?)
    }
    async fn report(&self, req: ApiRequest) -> Result<ApiResponse, ApiError> {
        let _ = self.admin(&req).await?;
        let from = req
            .query
            .get("from")
            .map(String::as_str)
            .unwrap_or("1970-01-01");
        let to = req
            .query
            .get("to")
            .map(String::as_str)
            .unwrap_or("2999-12-31");
        let g = req
            .query
            .get("group_by")
            .map(String::as_str)
            .unwrap_or("day");
        let col = match g {
            "status" => "o.status",
            "user" => "o.user_id::text",
            _ => "date(o.created_at)::text",
        };
        let from_date = report_date(from)?;
        let to_date = report_date(to)?;
        let sql=format!("SELECT {} bucket,COUNT(*) orders,COALESCE(SUM(o.total),0) revenue FROM orders o WHERE o.created_at >= $1 AND o.created_at < $2 GROUP BY {} ORDER BY 1",col,col);
        let rows = sqlx::query(&sql)
            .bind(from_date)
            .bind(to_date)
            .fetch_all(&self.context.pool)
            .await
            .map_err(|error| {
                tracing::error!(?error, "sales report grouping query failed");
                ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
            })?;
        let top_rows = sqlx::query(
            "SELECT p.name, COALESCE(SUM(i.price * i.quantity), 0) AS revenue
             FROM orders o
             JOIN order_items i ON i.order_id = o.id
             JOIN products p ON p.id = i.product_id
             WHERE o.created_at >= $1 AND o.created_at < $2
               AND o.status IN ('paid', 'shipped')
             GROUP BY p.name ORDER BY revenue DESC LIMIT 10",
        )
        .bind(from_date)
        .bind(to_date)
        .fetch_all(&self.context.pool)
        .await
        .map_err(|error| {
            tracing::error!(?error, "sales report top-products query failed");
            ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "database error")
        })?;
        let top_products = top_rows
            .into_iter()
            .map(|r| json!({"name": r.get::<String, _>("name"), "revenue": r.get::<f64, _>("revenue")}))
            .collect::<Vec<_>>();
        ok(
            json!({"from":from,"to":to,"group_by":g,"buckets":rows.into_iter().map(|r|json!({"bucket":r.get::<String,_>("bucket"),"orders":r.get::<i64,_>("orders"),"revenue":r.get::<f64,_>("revenue")})).collect::<Vec<_>>(),"top_products":top_products}),
        )
    }
}
