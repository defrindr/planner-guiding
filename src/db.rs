use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::config::Config;

pub async fn connect(config: &Config) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?)
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}
