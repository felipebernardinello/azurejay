mod grammar_check;
mod save_correction;
mod update_profile;
mod web_search;

use std::sync::Arc;

use crate::coa::{ToolAgent, ToolRegistry, registry_from};
use serde_json::{Map, Value};
use sqlx::PgPool;

pub use grammar_check::GrammarCheckAgent;
pub use save_correction::SaveCorrectionAgent;
pub use update_profile::UpdateProfileAgent;
pub use web_search::WebSearchAgent;

use crate::config::Config;
use crate::database::store::MemoryStore;

#[must_use]
pub fn build_registry(
    config: &Config,
    http: reqwest::Client,
    db: PgPool,
    store: MemoryStore,
) -> ToolRegistry {
    let agents: Vec<Arc<dyn ToolAgent>> = vec![
        Arc::new(WebSearchAgent::new(http.clone(), config.tavily_api_key.clone())),
        Arc::new(GrammarCheckAgent::new(
            http,
            &config.languagetool_base_url,
            config.languagetool_api_key.clone(),
        )),
        Arc::new(UpdateProfileAgent::new(db.clone(), store.clone())),
        Arc::new(SaveCorrectionAgent::new(db, store)),
    ];
    registry_from(agents)
}

pub(crate) fn parse_json_body(body: &str) -> Map<String, Value> {
    let body = body.trim();
    if body.is_empty() {
        return Map::new();
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(map)) => map,
        Ok(other) => {
            let mut map = Map::new();
            map.insert("value".into(), other);
            map
        }
        Err(_) => {
            let mut map = Map::new();
            map.insert("_raw".into(), Value::String(body.to_string()));
            map
        }
    }
}
