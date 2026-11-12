use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::{FromRow, PgPool, Postgres, Transaction};

use crate::{config::Config, domain::hash_password};

static SESSIONS: OnceLock<Mutex<crate::domain::SessionStore>> = OnceLock::new();

pub fn sessions() -> &'static Mutex<crate::domain::SessionStore> {
    SESSIONS.get_or_init(|| Mutex::new(crate::domain::SessionStore::default()))
}

pub fn clear_sessions() {
    if let Ok(mut sessions) = sessions().lock() {
        sessions.clear();
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct UserRecord {
    pub id: i32,
    pub username: Option<String>,
    pub email: Option<String>,
    pub name: Option<String>,
    pub password_hash: Option<String>,
    pub role: Option<String>,
    pub wallet_balance: Option<f64>,
    pub created_at: Option<NaiveDateTime>,
}

pub async fn user_by_id(pool: &PgPool, user_id: i32) -> Result<Option<UserRecord>> {
    Ok(sqlx::query_as::<_, UserRecord>("SELECT id, username, email, name, password_hash, role, wallet_balance, created_at FROM users WHERE id = $1")
        .bind(user_id).fetch_optional(pool).await?)
}

pub async fn authenticate_user(
    pool: &PgPool,
    username: &str,
    password: &str,
) -> Result<Option<UserRecord>> {
    Ok(sqlx::query_as::<_, UserRecord>("SELECT id, username, email, name, password_hash, role, wallet_balance, created_at FROM users WHERE username = $1 AND password_hash = $2")
        .bind(username).bind(hash_password(password)).fetch_optional(pool).await?)
}

pub async fn insert_user(
    pool: &PgPool,
    username: &str,
    email: &str,
    name: &str,
    password: &str,
) -> Result<i32> {
    Ok(sqlx::query_scalar("INSERT INTO users (username, email, name, password_hash, role, wallet_balance) VALUES ($1, $2, $3, $4, 'user', 0) RETURNING id")
        .bind(username).bind(email).bind(name).bind(hash_password(password)).fetch_one(pool).await?)
}

pub async fn seed(pool: &PgPool, config: &Config) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (username, email, name, password_hash, role, wallet_balance) \
         SELECT $1, $2, $3, $4, 'admin', 0 WHERE NOT EXISTS (SELECT 1 FROM users WHERE username = $1)",
    )
    .bind(&config.admin_user)
    .bind("admin@shopcore.local")
    .bind("Administrator")
    .bind(hash_password(&config.admin_password))
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT INTO coupons (code, percent, expires, single_use) \
         SELECT 'SAVE10', 10, '2099-12-31', FALSE \
         WHERE NOT EXISTS (SELECT 1 FROM coupons WHERE code = 'SAVE10')",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO coupons (code, percent, expires, single_use) \
         SELECT 'ONCE50', 50, '2099-12-31', TRUE \
         WHERE NOT EXISTS (SELECT 1 FROM coupons WHERE code = 'ONCE50')",
    )
    .execute(pool)
    .await?;

    let product_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await?;
    if product_count == 0 {
        for (sku, name, price, stock) in PRODUCTS {
            sqlx::query("INSERT INTO products (sku, name, price, stock) VALUES ($1, $2, $3, $4)")
                .bind(sku)
                .bind(name)
                .bind(price)
                .bind(stock)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn insert_returning_id<'a>(tx: &mut Transaction<'a, Postgres>, sql: &str) -> Result<i32> {
    Ok(sqlx::query_scalar::<_, i32>(sql)
        .fetch_one(&mut **tx)
        .await?)
}

const PRODUCTS: &[(&str, &str, f64, i32)] = &[
    ("SKU-1001", "Mechanical Keyboard", 89.99, 100),
    ("SKU-1002", "Wireless Mouse", 24.5, 100),
    ("SKU-1003", "USB-C Hub", 39.0, 100),
    ("SKU-1004", "27in Monitor", 229.0, 100),
    ("SKU-1005", "Laptop Stand", 32.75, 100),
    ("SKU-1006", "Webcam 1080p", 59.9, 100),
    ("SKU-1007", "Noise Cancelling Headphones", 149.0, 100),
    ("SKU-1008", "Desk Lamp", 19.99, 100),
    ("SKU-1009", "Ergonomic Chair", 319.0, 100),
    ("SKU-1010", "Cable Organizer", 7.25, 100),
];
