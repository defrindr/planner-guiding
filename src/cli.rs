use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::{application, config::Config, db, http, repository};

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

pub async fn run() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::from_env()?;

    match cli.command.unwrap_or(Command::Serve) {
        Command::InitDb => init_db(&config).await?,
        Command::Seed => seed(&config).await?,
        Command::PurgeSessions => purge_sessions(),
        Command::ImportProducts { path } => import_products(&config, &path).await?,
        Command::ExportSales { path } => export_sales(&config, &path).await?,
        Command::Serve => serve(&config).await?,
    }
    Ok(())
}

async fn init_db(config: &Config) -> Result<()> {
    let pool = db::connect(config).await?;
    db::migrate(&pool).await?;
    println!("tables ready");
    Ok(())
}

async fn seed(config: &Config) -> Result<()> {
    let pool = db::connect(config).await?;
    db::migrate(&pool).await?;
    repository::seed(&pool, config).await?;
    println!("seeded");
    Ok(())
}

fn purge_sessions() {
    repository::clear_sessions();
    println!("purged sessions");
}

async fn import_products(config: &Config, path: &str) -> Result<()> {
    let pool = db::connect(config).await?;
    db::migrate(&pool).await?;
    let text = std::fs::read_to_string(path)?;
    let mut count = 0;
    for line in text.lines().skip(1) {
        let fields: Vec<_> = line.split(',').collect();
        if fields.len() < 4 {
            continue;
        }
        let exists = repository::products::exists_by_sku(&pool, fields[0]).await?;
        if !exists {
            repository::products::insert(
                &pool,
                fields[0],
                fields[1],
                fields[2].parse::<f64>().unwrap_or(0.0),
                fields[3].parse::<i32>().unwrap_or(0),
            )
            .await?;
            count += 1;
        }
    }
    println!("imported {count} products");
    Ok(())
}

async fn export_sales(config: &Config, path: &str) -> Result<()> {
    let pool = db::connect(config).await?;
    db::migrate(&pool).await?;
    let rows = repository::orders::all_for_export(&pool).await?;
    let mut out = String::from("order_no,status,total,created_at\n");
    for row in rows {
        out.push_str(&format!(
            "{},{},{},{}\n",
            row.order_no, row.status, row.total, row.created_at
        ));
    }
    std::fs::write(path, out)?;
    println!("wrote {path}");
    Ok(())
}

async fn serve(config: &Config) -> Result<()> {
    let pool = db::connect(config).await?;
    db::migrate(&pool).await?;
    let app = http::router(std::sync::Arc::new(application::ShopCore::new(
        pool,
        config.clone(),
    )));
    tracing::info!(bind = %config.bind, "ShopCore HTTP service started");
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
