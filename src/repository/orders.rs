use chrono::NaiveDateTime;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct OrderDetailRecord {
    pub id: i32,
    pub order_no: i32,
    pub user_id: i32,
    pub status: String,
    pub total: f64,
    pub paid_amount: f64,
    pub refunded: f64,
    pub coupon: Option<String>,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderItemRecord {
    pub product_id: i32,
    pub quantity: i32,
    pub price: f64,
    pub sku: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderForPayment {
    pub user_id: i32,
    pub status: String,
    pub total: f64,
    pub order_no: i32,
    pub notify_url: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderForCancel {
    pub user_id: i32,
    pub status: String,
    pub total: f64,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderForRefund {
    pub user_id: i32,
    pub status: String,
    pub total: f64,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderStatusRecord {
    pub id: i32,
    pub order_no: i32,
    pub status: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct OrderExportRecord {
    pub order_no: i32,
    pub status: String,
    pub total: f64,
    pub created_at: NaiveDateTime,
}

pub async fn next_order_no<'e, E>(executor: E) -> sqlx::Result<i32>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, i32>("SELECT COALESCE(MAX(order_no),$1)+1 FROM orders")
        .bind(crate::domain::DEFAULT_ORDER_NO as i32)
        .fetch_one(executor)
        .await
}

pub async fn insert<'e, E>(
    executor: E,
    order_no: i32,
    user_id: i32,
    coupon: Option<&str>,
    notify_url: Option<&str>,
) -> sqlx::Result<i32>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, i32>(
        "INSERT INTO orders(order_no,user_id,status,total,paid_amount,refunded,coupon,notify_url,created_at) \
         VALUES($1,$2,'pending',0,0,0,$3,$4,now()) RETURNING id",
    )
    .bind(order_no)
    .bind(user_id)
    .bind(coupon)
    .bind(notify_url)
    .fetch_one(executor)
    .await
}

pub async fn set_total<'e, E>(executor: E, order_id: i32, total: f64) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE orders SET total=$1 WHERE id=$2")
        .bind(total)
        .bind(order_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn list_ids_for_user<'e, E>(
    executor: E,
    user_id: i32,
    limit: i64,
    offset: i64,
) -> sqlx::Result<Vec<i32>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar::<_, i32>(
        "SELECT id FROM orders WHERE user_id=$1 ORDER BY id DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(executor)
    .await
}

pub async fn detail<'e, E>(executor: E, order_id: i32) -> sqlx::Result<Option<OrderDetailRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderDetailRecord>(
        "SELECT id,order_no,user_id,status,total,paid_amount,refunded,coupon,created_at \
         FROM orders WHERE id=$1",
    )
    .bind(order_id)
    .fetch_optional(executor)
    .await
}

pub async fn items<'e, E>(executor: E, order_id: i32) -> sqlx::Result<Vec<OrderItemRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderItemRecord>(
        "SELECT i.product_id,i.quantity,i.price,p.sku,p.name FROM order_items i \
         LEFT JOIN products p ON p.id=i.product_id WHERE i.order_id=$1 ORDER BY i.id",
    )
    .bind(order_id)
    .fetch_all(executor)
    .await
}

pub async fn insert_item<'e, E>(
    executor: E,
    order_id: i32,
    product_id: i32,
    quantity: i32,
    price: f64,
) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("INSERT INTO order_items(order_id,product_id,quantity,price) VALUES($1,$2,$3,$4)")
        .bind(order_id)
        .bind(product_id)
        .bind(quantity)
        .bind(price)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn restock_items<'e, E>(executor: E, order_id: i32) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "UPDATE products p SET stock=p.stock+i.quantity FROM order_items i \
         WHERE i.order_id=$1 AND i.product_id=p.id",
    )
    .bind(order_id)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn for_payment<'e, E>(executor: E, order_id: i32) -> sqlx::Result<Option<OrderForPayment>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderForPayment>(
        "SELECT user_id,status,total,order_no,notify_url FROM orders WHERE id=$1",
    )
    .bind(order_id)
    .fetch_optional(executor)
    .await
}

pub async fn for_cancel<'e, E>(executor: E, order_id: i32) -> sqlx::Result<Option<OrderForCancel>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderForCancel>("SELECT user_id,status,total FROM orders WHERE id=$1")
        .bind(order_id)
        .fetch_optional(executor)
        .await
}

pub async fn for_refund<'e, E>(executor: E, order_id: i32) -> sqlx::Result<Option<OrderForRefund>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderForRefund>("SELECT user_id,status,total FROM orders WHERE id=$1")
        .bind(order_id)
        .fetch_optional(executor)
        .await
}

pub async fn mark_paid<'e, E>(executor: E, order_id: i32) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE orders SET status='paid',paid_amount=total WHERE id=$1")
        .bind(order_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn mark_paid_if_pending<'e, E>(executor: E, order_id: i32) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query(
        "UPDATE orders SET status='paid',paid_amount=total WHERE id=$1 AND status='pending'",
    )
    .bind(order_id)
    .execute(executor)
    .await?;
    Ok(())
}

pub async fn mark_cancelled<'e, E>(executor: E, order_id: i32) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE orders SET status='cancelled' WHERE id=$1")
        .bind(order_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn apply_refund<'e, E>(executor: E, order_id: i32, amount: f64) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE orders SET status='refunded',refunded=COALESCE(refunded,0)+$1 WHERE id=$2")
        .bind(amount)
        .bind(order_id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn update_status<'e, E>(
    executor: E,
    order_id: i32,
    status: &str,
) -> sqlx::Result<Option<OrderStatusRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderStatusRecord>(
        "UPDATE orders SET status=$1 WHERE id=$2 RETURNING id,order_no,status",
    )
    .bind(status)
    .bind(order_id)
    .fetch_optional(executor)
    .await
}

pub async fn all_for_export<'e, E>(executor: E) -> sqlx::Result<Vec<OrderExportRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, OrderExportRecord>(
        "SELECT order_no,status,total,created_at FROM orders ORDER BY id",
    )
    .fetch_all(executor)
    .await
}
