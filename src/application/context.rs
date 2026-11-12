use reqwest::Client;
use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct ApplicationContext {
    pub pool: PgPool,
    pub config: Config,
    pub http_client: Client,
}

impl ApplicationContext {
    pub fn new(pool: PgPool, config: Config) -> Self {
        Self {
            pool,
            config,
            http_client: Client::new(),
        }
    }
}
