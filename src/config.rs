use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Config {
    pub api_host: String,
    pub api_port: u16,

    pub database_url: String,
    pub redis_url: String,

    pub jwt_secret: String,
    pub access_token_expiry: Duration,

    pub groq_api_key: String,
    pub coa_model: String,
    pub coa_tts_n: usize,

    pub tavily_api_key: Option<String>,
    pub languagetool_base_url: String,
    pub languagetool_api_key: Option<String>,

    pub eleven_labs_api_key: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),
    #[error("environment variable {0} is not valid: {1}")]
    Invalid(&'static str, String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            api_host: opt("API_HOST").unwrap_or_else(|| "0.0.0.0".into()),
            api_port: parse_opt("API_PORT", 8000)?,

            database_url: req("DATABASE_URL")?,
            redis_url: opt("REDIS_URL").unwrap_or_else(|| "redis://localhost:6379/0".into()),

            jwt_secret: req("SECRET_KEY")?,
            access_token_expiry: Duration::from_secs(
                60 * parse_opt::<u64>("ACCESS_TOKEN_EXPIRE_MINUTES", 30)?,
            ),

            groq_api_key: req("GROQ_API_KEY")?,
            coa_model: opt("COA_MODEL").unwrap_or_else(|| "qwen/qwen3-32b".into()),
            coa_tts_n: parse_opt("COA_TTS_N", 3)?,

            tavily_api_key: opt("TAVILY_API_KEY"),
            languagetool_base_url: opt("LANGUAGETOOL_BASE_URL")
                .unwrap_or_else(|| "https://api.languagetoolplus.com/v2".into()),
            languagetool_api_key: opt("LANGUAGETOOL_API_KEY"),

            eleven_labs_api_key: opt("ELEVEN_LABS_API_KEY"),
        })
    }
}

fn req(key: &'static str) -> Result<String, ConfigError> {
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => Ok(v),
        _ => Err(ConfigError::Missing(key)),
    }
}

fn opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn parse_opt<T>(key: &'static str, default: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) if !v.is_empty() => v
            .parse::<T>()
            .map_err(|e| ConfigError::Invalid(key, e.to_string())),
        _ => Ok(default),
    }
}
