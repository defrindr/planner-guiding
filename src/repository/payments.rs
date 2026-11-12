pub async fn insert_captured<'e, E>(
    executor: E,
    order_id: i32,
    amount: f64,
    bank_ref: &str,
) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "INSERT INTO payments(order_id,amount,method,bank_ref,status) \
         VALUES($1,$2,'bank',$3,'captured')",
    )
    .bind(order_id)
    .bind(amount)
    .bind(bank_ref)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn settle<'e, E>(executor: E, order_id: i32, bank_ref: &str) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE payments SET status='settled' WHERE order_id=$1 AND bank_ref=$2")
        .bind(order_id)
        .bind(bank_ref)
        .execute(executor)
        .await?;
    Ok(())
}
