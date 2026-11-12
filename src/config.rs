use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bank_url: String,
    pub bank_secret: String,
    pub export_dir: PathBuf,
    pub admin_user: String,
    pub admin_password: String,
    pub bind: SocketAddr,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = env::var("BIND").unwrap_or_else(|_| {
            format!("{}:{}", env_or("HOST", "0.0.0.0"), env_or("PORT", "8000"))
        });
        Ok(Self {
            database_url: env_or("DATABASE_URL", "postgresql://shop:shop@localhost:5432/shop"),
            bank_url: env_or("BANK_URL", "http://localhost:5001"),
            bank_secret: env_or("BANK_SECRET", "bank-shared-secret-2019"),
            export_dir: PathBuf::from(env_or("EXPORT_DIR", "/tmp/exports")),
            admin_user: env_or("ADMIN_USER", "admin"),
            admin_password: env_or("ADMIN_PASSWORD", "admin123"),
            bind: bind
                .parse()
                .with_context(|| format!("invalid BIND address: {bind}"))?,
        })
    }
}

fn env_or(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
