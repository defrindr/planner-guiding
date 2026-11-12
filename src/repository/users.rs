use anyhow::Result;
use chrono::NaiveDateTime;
use sqlx::FromRow;

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

#[derive(Debug, Clone, FromRow)]
pub struct NameEmailRecord {
    pub name: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct RoleRecord {
    pub id: i32,
    pub username: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum ProfileField {
    Username,
    Email,
    Name,
}

impl ProfileField {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Username => "username",
            Self::Email => "email",
            Self::Name => "name",
        }
    }
}

pub async fn user_by_id<'e, E>(executor: E, user_id: i32) -> Result<Option<UserRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_as::<_, UserRecord>("SELECT id, username, email, name, password_hash, role, wallet_balance, created_at FROM users WHERE id = $1")
        .bind(user_id).fetch_optional(executor).await?)
}

pub async fn authenticate_user<'e, E>(
    executor: E,
    username: &str,
    password: &str,
) -> Result<Option<UserRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_as::<_, UserRecord>("SELECT id, username, email, name, password_hash, role, wallet_balance, created_at FROM users WHERE username = $1 AND password_hash = $2")
        .bind(username).bind(hash_password(password)).fetch_optional(executor).await?)
}

pub async fn insert_user<'e, E>(
    executor: E,
    username: &str,
    email: &str,
    name: &str,
    password: &str,
) -> Result<i32>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(sqlx::query_scalar("INSERT INTO users (username, email, name, password_hash, role, wallet_balance) VALUES ($1, $2, $3, $4, 'user', 0) RETURNING id")
        .bind(username).bind(email).bind(name).bind(hash_password(password)).fetch_one(executor).await?)
}

pub async fn user_by_username_or_email<'e, E>(
    executor: E,
    username: &str,
    email: &str,
) -> sqlx::Result<Option<i32>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE username=$1 OR email=$2")
        .bind(username)
        .bind(email)
        .fetch_optional(executor)
        .await
}

pub async fn update_profile_field<'e, E>(
    executor: E,
    user_id: i32,
    field: ProfileField,
    value: &str,
) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let sql = format!("UPDATE users SET {}=$1 WHERE id=$2", field.as_sql());
    sqlx::query(&sql)
        .bind(value)
        .bind(user_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn list_users<'e, E>(executor: E) -> sqlx::Result<Vec<UserRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, UserRecord>(
        "SELECT id,username,email,name,role,wallet_balance,password_hash,created_at \
         FROM users ORDER BY id",
    )
    .fetch_all(executor)
    .await
}

pub async fn update_role<'e, E>(
    executor: E,
    user_id: i32,
    role: &str,
) -> sqlx::Result<Option<RoleRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, RoleRecord>(
        "UPDATE users SET role=$1 WHERE id=$2 RETURNING id,username,role",
    )
    .bind(if role.is_empty() { "user" } else { role })
    .bind(user_id)
    .fetch_optional(executor)
    .await
}

pub async fn name_email<'e, E>(executor: E, user_id: i32) -> sqlx::Result<Option<NameEmailRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, NameEmailRecord>("SELECT name,email FROM users WHERE id=$1")
        .bind(user_id)
        .fetch_optional(executor)
        .await
}
