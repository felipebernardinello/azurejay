use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub user_id: String,
    pub conversation_id: String,
}

impl ToolContext {
    #[must_use]
    pub fn new(user_id: impl Into<String>, conversation_id: impl Into<String>) -> Self {
        Self { user_id: user_id.into(), conversation_id: conversation_id.into() }
    }
}

#[async_trait]
pub trait ToolAgent: Send + Sync {
    fn name(&self) -> &str;

    async fn run(&self, body: &str, ctx: &ToolContext) -> String;
}

pub type ToolRegistry = HashMap<String, Arc<dyn ToolAgent>>;

#[must_use]
pub fn registry_from(agents: Vec<Arc<dyn ToolAgent>>) -> ToolRegistry {
    agents
        .into_iter()
        .map(|a| (a.name().to_string(), a))
        .collect()
}
