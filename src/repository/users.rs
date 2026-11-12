use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::{FromRow, PgPool};

use crate::domain::hash_password;

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
