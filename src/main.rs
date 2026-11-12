mod application;
mod cli;
mod config;
mod db;
mod domain;
mod http;
mod repository;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "shopcore=info".into()))
        .init();

    cli::run().await
}
