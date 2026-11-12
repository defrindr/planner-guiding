use chrono::{NaiveDate, NaiveDateTime};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct SalesExportRow {
    pub order_no: i32,
    pub created_at: NaiveDateTime,
    pub status: String,
    pub username: String,
    pub product: String,
    pub quantity: i32,
    pub price: f64,
}

#[derive(Debug, Clone, FromRow)]
pub struct SalesBucketRow {
    pub bucket: String,
    pub orders: i64,
    pub revenue: f64,
}

#[derive(Debug, Clone, FromRow)]
pub struct TopProductRow {
    pub name: String,
    pub revenue: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum ReportGroup {
    Day,
    Status,
    User,
}

pub async fn export_rows<'e, E>(
    executor: E,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> sqlx::Result<Vec<SalesExportRow>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, SalesExportRow>(
        "SELECT o.order_no,o.created_at,o.status,u.username,p.name AS product,i.quantity,i.price \
         FROM orders o JOIN users u ON u.id=o.user_id JOIN order_items i ON i.order_id=o.id \
         JOIN products p ON p.id=i.product_id \
         WHERE o.created_at >= $1 AND o.created_at < $2 ORDER BY o.id",
    )
    .bind(from_date)
    .bind(to_date)
    .fetch_all(executor)
    .await
}

pub async fn buckets<'e, E>(
    executor: E,
    group: ReportGroup,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> sqlx::Result<Vec<SalesBucketRow>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let sql = match group {
        ReportGroup::Day => {
            "SELECT date(o.created_at)::text bucket,COUNT(*) orders,\
             COALESCE(SUM(o.total),0) revenue FROM orders o \
             WHERE o.created_at >= $1 AND o.created_at < $2 \
             GROUP BY date(o.created_at)::text ORDER BY 1"
        }
        ReportGroup::Status => {
            "SELECT o.status bucket,COUNT(*) orders,COALESCE(SUM(o.total),0) revenue \
             FROM orders o WHERE o.created_at >= $1 AND o.created_at < $2 \
             GROUP BY o.status ORDER BY 1"
        }
        ReportGroup::User => {
            "SELECT o.user_id::text bucket,COUNT(*) orders,COALESCE(SUM(o.total),0) revenue \
             FROM orders o WHERE o.created_at >= $1 AND o.created_at < $2 \
             GROUP BY o.user_id::text ORDER BY 1"
        }
    };
    sqlx::query_as::<_, SalesBucketRow>(sql)
        .bind(from_date)
        .bind(to_date)
        .fetch_all(executor)
        .await
}

pub async fn top_products<'e, E>(
    executor: E,
    from_date: NaiveDate,
    to_date: NaiveDate,
) -> sqlx::Result<Vec<TopProductRow>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, TopProductRow>(
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
    .fetch_all(executor)
    .await
}
