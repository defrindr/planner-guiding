use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct WalletRecord {
    pub id: i32,
    pub wallet_balance: Option<f64>,
}

pub async fn balance_for_update<'e, E>(executor: E, user_id: i32) -> sqlx::Result<f64>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, f64>("SELECT wallet_balance FROM users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_one(executor)
        .await
}

pub async fn for_update_one<'e, E>(executor: E, user_id: i32) -> sqlx::Result<WalletRecord>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, WalletRecord>("SELECT id,wallet_balance FROM users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_one(executor)
        .await
}

pub async fn for_update<'e, E>(executor: E, user_id: i32) -> sqlx::Result<Option<WalletRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, WalletRecord>("SELECT id,wallet_balance FROM users WHERE id=$1 FOR UPDATE")
        .bind(user_id)
        .fetch_optional(executor)
        .await
}

pub async fn topup<'e, E>(executor: E, user_id: i32, amount: f64) -> sqlx::Result<f64>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, f64>(
        "UPDATE users SET wallet_balance=COALESCE(wallet_balance,0)+$1 WHERE id=$2 \
         RETURNING wallet_balance",
    )
    .bind(amount)
    .bind(user_id)
    .fetch_one(executor)
    .await
}

pub async fn credit<'e, E>(executor: E, user_id: i32, amount: f64) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE users SET wallet_balance=COALESCE(wallet_balance,0)+$1 WHERE id=$2")
        .bind(amount)
        .bind(user_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn debit<'e, E>(executor: E, user_id: i32, amount: f64) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE users SET wallet_balance=wallet_balance-$1 WHERE id=$2")
        .bind(amount)
        .bind(user_id)
        .execute(executor)
        .await?;
    Ok(())
}
