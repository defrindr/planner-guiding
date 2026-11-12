use chrono::NaiveDateTime;
use sqlx::FromRow;

pub const PRODUCTS: &[(&str, &str, f64, i32)] = &[
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

#[derive(Debug, Clone, FromRow)]
pub struct ProductRecord {
    pub id: i32,
    pub sku: Option<String>,
    pub name: Option<String>,
    pub price: Option<f64>,
    pub stock: Option<i32>,
    pub created_at: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Copy)]
pub enum ProductSort {
    Id,
    Price,
    Name,
    Sku,
    Stock,
}

impl ProductSort {
    fn as_sql(self) -> &'static str {
        match self {
            Self::Id => "id",
            Self::Price => "price",
            Self::Name => "name",
            Self::Sku => "sku",
            Self::Stock => "stock",
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ProductForOrder {
    pub sku: String,
    pub price: f64,
    pub stock: i32,
}

pub async fn list<'e, E>(
    executor: E,
    query: Option<&str>,
    sort: ProductSort,
    page: i64,
    limit: i64,
) -> sqlx::Result<Vec<ProductRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let mut sql = format!(
        "SELECT id,sku,name,price,stock,created_at FROM products {} ORDER BY {}",
        if query.is_some() {
            "WHERE name ILIKE $1"
        } else {
            ""
        },
        sort.as_sql()
    );
    if limit >= 0 {
        if query.is_some() {
            sql.push_str(" LIMIT $2 OFFSET $3");
        } else {
            sql.push_str(" LIMIT $1 OFFSET $2");
        }
    }
    let mut statement = sqlx::query_as::<_, ProductRecord>(&sql);
    if let Some(query) = query {
        statement = statement.bind(format!("%{}%", query));
    }
    if limit >= 0 {
        statement = statement.bind(limit).bind((page - 1) * limit);
    }
    statement.fetch_all(executor).await
}

pub async fn by_id<'e, E>(executor: E, id: i32) -> sqlx::Result<Option<ProductRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, ProductRecord>(
        "SELECT id,sku,name,price,stock,created_at FROM products WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
}

pub async fn for_order<'e, E>(executor: E, id: i32) -> sqlx::Result<Option<ProductForOrder>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, ProductForOrder>(
        "SELECT sku,price,stock FROM products WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(executor)
    .await
}

pub async fn insert<'e, E>(
    executor: E,
    sku: &str,
    name: &str,
    price: f64,
    stock: i32,
) -> sqlx::Result<ProductRecord>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, ProductRecord>(
        "INSERT INTO products(sku,name,price,stock) VALUES($1,$2,$3,$4) \
         RETURNING id,sku,name,price,stock,created_at",
    )
    .bind(sku)
    .bind(name)
    .bind(price)
    .bind(stock)
    .fetch_one(executor)
    .await
}

pub async fn update<'e, E>(
    executor: E,
    id: i32,
    sku: &str,
    name: &str,
    price: f64,
    stock: i32,
) -> sqlx::Result<Option<ProductRecord>>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_as::<_, ProductRecord>(
        "UPDATE products SET sku=$1,name=$2,price=$3,stock=$4 WHERE id=$5 \
         RETURNING id,sku,name,price,stock,created_at",
    )
    .bind(sku)
    .bind(name)
    .bind(price)
    .bind(stock)
    .bind(id)
    .fetch_optional(executor)
    .await
}

pub async fn decrement_stock<'e, E>(executor: E, id: i32, quantity: i32) -> sqlx::Result<()>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query("UPDATE products SET stock=stock-$1 WHERE id=$2")
        .bind(quantity)
        .bind(id)
        .execute(executor)
        .await?;
    Ok(())
}

pub async fn exists_by_sku<'e, E>(executor: E, sku: &str) -> sqlx::Result<bool>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    Ok(
        sqlx::query_scalar::<_, i32>("SELECT id FROM products WHERE sku=$1")
            .bind(sku)
            .fetch_optional(executor)
            .await?
            .is_some(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_sort_maps_to_whitelisted_sql_columns() {
        assert_eq!(ProductSort::Id.as_sql(), "id");
        assert_eq!(ProductSort::Price.as_sql(), "price");
        assert_eq!(ProductSort::Name.as_sql(), "name");
        assert_eq!(ProductSort::Sku.as_sql(), "sku");
        assert_eq!(ProductSort::Stock.as_sql(), "stock");
    }
}
