use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;

use crate::auth::RateLimiter;
use crate::coa::{self, CoA, CoAConfig};
use crate::config::Config;
use crate::database::{self, MemoryStore};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub store: MemoryStore,
    pub coa: Arc<CoA>,
    pub http: reqwest::Client,
    pub register_limiter: RateLimiter,
}

impl AppState {
    pub async fn build(config: Config) -> anyhow::Result<Self> {
        let db = database::connect(&config.database_url).await?;
        database::migrate(&db).await?;

        let store = MemoryStore::connect(&config.redis_url).await?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("azurejay/1.0")
            .build()?;

        let tools = coa::build_registry(&config, http.clone(), db.clone(), store.clone());
        let coa = Arc::new(CoA::new(
            &config.groq_api_key,
            &config.coa_model,
            tools,
            CoAConfig::default(),
            config.coa_tts_n,
        )?);
        tracing::info!(model = %config.coa_model, tts_n = config.coa_tts_n, "CoA ready");

        Ok(Self {
            config: Arc::new(config),
            db,
            store,
            coa,
            http,
            register_limiter: RateLimiter::new(5, Duration::from_secs(3600)),
        })
    }
}
