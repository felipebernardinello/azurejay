use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            let level = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into());
            EnvFilter::try_new(format!("azurejay={level},coa={level},tower_http=info"))
        })
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let json = std::env::var("LOG_FORMAT").is_ok_and(|v| v.eq_ignore_ascii_case("json"));

    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(fmt::layer().json()).init();
    } else {
        registry.with(fmt::layer().compact()).init();
    }
}
