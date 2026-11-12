use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct CouponRecord {
    pub percent: i32,
    pub expires: String,
    pub single_use: bool,
    pub used_count: i32,
}

pub async fn by_code<'e, E>(executor: E, code: &str) -> sqlx::Result<Option<CouponRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, CouponRecord>(
        "SELECT percent,expires,single_use,used_count FROM coupons WHERE code=$1",
    )
    .bind(code)
    .fetch_optional(executor)
    .await
}

pub async fn increment_usage<'e, E>(executor: E, code: &str) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE coupons SET used_count=used_count+1 WHERE code=$1")
        .bind(code)
        .execute(executor)
        .await?;
    Ok(())
}
