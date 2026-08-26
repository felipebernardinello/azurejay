use async_trait::async_trait;
use crate::coa::{ToolAgent, ToolContext};
use serde::Deserialize;
use serde_json::json;

const TAVILY_URL: &str = "https://api.tavily.com/search";

pub struct WebSearchAgent {
    http: reqwest::Client,
    api_key: Option<String>,
}

impl WebSearchAgent {
    #[must_use]
    pub fn new(http: reqwest::Client, api_key: Option<String>) -> Self {
        Self { http, api_key }
    }

    async fn search(&self, query: &str, api_key: &str) -> Result<String, reqwest::Error> {
        let resp: TavilyResponse = self
            .http
            .post(TAVILY_URL)
            .json(&json!({
                "api_key": api_key,
                "query": query,
                "max_results": 3,
                "include_answer": false,
                "include_raw_content": false,
            }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if resp.results.is_empty() {
            return Ok("No relevant information.".to_string());
        }
        let lines: Vec<String> = resp
            .results
            .iter()
            .map(|r| format!("[{}]({})\n{}", r.title, r.url, r.content))
            .collect();
        Ok(lines.join("\n\n"))
    }
}

#[async_trait]
impl ToolAgent for WebSearchAgent {
    fn name(&self) -> &str {
        "web_search"
    }

    async fn run(&self, body: &str, _ctx: &ToolContext) -> String {
        let query = body.trim();
        if query.is_empty() {
            return "No relevant information. (empty query)".to_string();
        }
        let Some(api_key) = &self.api_key else {
            return "No relevant information. (web search is not configured)".to_string();
        };
        match self.search(query, api_key).await {
            Ok(observation) => observation,
            Err(err) => {
                tracing::warn!(%err, "web_search failed");
                format!("Search error: {err}")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct TavilyResponse {
    #[serde(default)]
    results: Vec<TavilyResult>,
}

#[derive(Debug, Deserialize)]
struct TavilyResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}
