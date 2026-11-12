use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use sqlx::{PgPool, Postgres, Transaction};

use crate::{config::Config, domain::hash_password};

pub mod products;
pub mod users;
pub use users::{authenticate_user, insert_user, user_by_id};

static SESSIONS: OnceLock<Mutex<crate::domain::SessionStore>> = OnceLock::new();

pub fn sessions() -> &'static Mutex<crate::domain::SessionStore> {
    SESSIONS.get_or_init(|| Mutex::new(crate::domain::SessionStore::default()))
}

pub fn clear_sessions() {
    if let Ok(mut sessions) = sessions().lock() {
        sessions.clear();
    }
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
        for (sku, name, price, stock) in products::PRODUCTS {
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
