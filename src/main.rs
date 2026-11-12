mod config;
mod core;
mod db;
mod domain;
mod http;
mod repository;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use sqlx::Row;

#[derive(Debug, Parser)]
#[command(name = "shopcore", version, about = "ShopCore Rust service")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create or update the PostgreSQL schema.
    InitDb,
    /// Start the HTTP service.
    Serve,
    /// Seed the administrator, coupons, and products.
    Seed,
    /// Clear the process-local authentication sessions.
    PurgeSessions,
    /// Import products from a CSV with sku,name,price,stock columns.
    ImportProducts { path: String },
    /// Export order summary CSV.
    ExportSales { path: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "shopcore=info".into()))
        .init();

    let cli = Cli::parse();
    let config = Config::from_env()?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::InitDb => {
            let pool = db::connect(&config).await?;
            db::migrate(&pool).await?;
            println!("tables ready");
        }
        Command::Seed => {
            let pool = db::connect(&config).await?;
            db::migrate(&pool).await?;
            repository::seed(&pool, &config).await?;
            println!("seeded");
        }
        Command::PurgeSessions => {
            // Sessions are intentionally process-local, matching the legacy service.
            repository::clear_sessions();
            println!("purged sessions");
        }
        Command::ImportProducts { path } => {
            let pool = db::connect(&config).await?;
            db::migrate(&pool).await?;
            let text = std::fs::read_to_string(path)?;
            let mut count = 0;
            for line in text.lines().skip(1) {
                let p: Vec<_> = line.split(',').collect();
                if p.len() < 4 {
                    continue;
                }
                let exists = sqlx::query("SELECT id FROM products WHERE sku=$1")
                    .bind(p[0])
                    .fetch_optional(&pool)
                    .await?
                    .is_some();
                if !exists {
                    sqlx::query("INSERT INTO products(sku,name,price,stock) VALUES($1,$2,$3,$4)")
                        .bind(p[0])
                        .bind(p[1])
                        .bind(p[2].parse::<f64>().unwrap_or(0.0))
                        .bind(p[3].parse::<i32>().unwrap_or(0))
                        .execute(&pool)
                        .await?;
                    count += 1;
                }
            }
            println!("imported {count} products");
        }
        Command::ExportSales { path } => {
            let pool = db::connect(&config).await?;
            db::migrate(&pool).await?;
            let rows =
                sqlx::query("SELECT order_no,status,total,created_at FROM orders ORDER BY id")
                    .fetch_all(&pool)
                    .await?;
            let mut out = String::from("order_no,status,total,created_at\n");
            for r in rows {
                out.push_str(&format!(
                    "{},{},{},{}\n",
                    r.get::<i32, _>("order_no"),
                    r.get::<String, _>("status"),
                    r.get::<f64, _>("total"),
                    r.get::<chrono::NaiveDateTime, _>("created_at")
                ));
            }
            std::fs::write(&path, out)?;
            println!("wrote {path}");
        }
        Command::Serve => {
            let pool = db::connect(&config).await?;
            db::migrate(&pool).await?;
            let app = http::router(std::sync::Arc::new(core::ShopCore::new(
                pool,
                config.clone(),
            )));
            tracing::info!(bind = %config.bind, "ShopCore HTTP service started");
            let listener = tokio::net::TcpListener::bind(&config.bind).await?;
            axum::serve(listener, app).await?;
        }
    }
    Ok(())
}
