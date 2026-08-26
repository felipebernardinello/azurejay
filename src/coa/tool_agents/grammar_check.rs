use async_trait::async_trait;
use crate::coa::{ToolAgent, ToolContext};
use serde::Deserialize;

pub struct GrammarCheckAgent {
    http: reqwest::Client,
    check_url: String,
    api_key: Option<String>,
}

impl GrammarCheckAgent {
    #[must_use]
    pub fn new(http: reqwest::Client, base_url: &str, api_key: Option<String>) -> Self {
        Self { http, check_url: format!("{base_url}/check"), api_key }
    }

    async fn check(&self, text: &str) -> Result<String, reqwest::Error> {
        let mut form = vec![
            ("text", text.to_string()),
            ("language", "en-US".to_string()),
            ("enabledOnly", "false".to_string()),
        ];
        if let Some(key) = &self.api_key {
            form.push(("apiKey", key.clone()));
        }

        let resp: LtResponse = self
            .http
            .post(&self.check_url)
            .form(&form)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        if resp.matches.is_empty() {
            return Ok("No syntax errors found.".to_string());
        }

        let mut corrected: Vec<char> = text.chars().collect();
        let mut matches = resp.matches.clone();
        matches.sort_by_key(|m| std::cmp::Reverse(m.offset));
        for m in &matches {
            if let Some(rep) = m.replacements.first() {
                let start = m.offset.min(corrected.len());
                let end = (m.offset + m.length).min(corrected.len());
                corrected.splice(start..end, rep.value.chars());
            }
        }
        let corrected: String = corrected.into_iter().collect();
        let explanation = explain(&resp.matches);

        Ok(format!(
            "Errors found in: '{text}'. Suggested correction: '{corrected}'. Details: {explanation}"
        ))
    }
}

#[async_trait]
impl ToolAgent for GrammarCheckAgent {
    fn name(&self) -> &str {
        "grammar_check"
    }

    async fn run(&self, body: &str, _ctx: &ToolContext) -> String {
        let text = body.trim();
        if text.is_empty() {
            return "No syntax errors found.".to_string();
        }
        match self.check(text).await {
            Ok(observation) => observation,
            Err(err) => format!("grammar check unavailable: {err}"),
        }
    }
}

fn explain(matches: &[LtMatch]) -> String {
    if matches.len() == 1 {
        let m = &matches[0];
        let label = if m.short_message.is_empty() { &m.message } else { &m.short_message };
        return match m.replacements.first() {
            Some(rep) => format!("Found 1 error: {label}. Suggested correction: '{}'", rep.value),
            None => format!("Found 1 error: {label}"),
        };
    }
    let mut lines = Vec::with_capacity(matches.len());
    for (i, m) in matches.iter().enumerate() {
        let label = if m.short_message.is_empty() { &m.message } else { &m.short_message };
        match m.replacements.first() {
            Some(rep) => lines.push(format!("{}. {label} -> '{}'", i + 1, rep.value)),
            None => lines.push(format!("{}. {label}", i + 1)),
        }
    }
    format!("Found {} errors:\n{}", matches.len(), lines.join("\n"))
}

#[derive(Debug, Clone, Deserialize)]
struct LtResponse {
    #[serde(default)]
    matches: Vec<LtMatch>,
}

#[derive(Debug, Clone, Deserialize)]
struct LtMatch {
    #[serde(default)]
    message: String,
    #[serde(default, rename = "shortMessage")]
    short_message: String,
    offset: usize,
    length: usize,
    #[serde(default)]
    replacements: Vec<LtReplacement>,
}

#[derive(Debug, Clone, Deserialize)]
struct LtReplacement {
    #[serde(default)]
    value: String,
}
