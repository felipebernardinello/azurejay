use async_trait::async_trait;
use crate::coa::{ToolAgent, ToolContext};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::parse_json_body;
use crate::entities::MessageRole;
use crate::database::store::MemoryStore;

const REQUIRED: [&str; 4] = ["original_text", "corrected_text", "explanation", "improvement"];

pub struct SaveCorrectionAgent {
    db: PgPool,
    store: MemoryStore,
}

impl SaveCorrectionAgent {
    #[must_use]
    pub fn new(db: PgPool, store: MemoryStore) -> Self {
        Self { db, store }
    }
}

#[async_trait]
impl ToolAgent for SaveCorrectionAgent {
    fn name(&self) -> &str {
        "save_correction"
    }

    async fn run(&self, body: &str, ctx: &ToolContext) -> String {
        let params = parse_json_body(body);

        let mut fields = [const { String::new() }; 4];
        let mut missing = Vec::new();
        for (slot, key) in fields.iter_mut().zip(REQUIRED) {
            match params.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                Some(v) => *slot = v.to_string(),
                None => missing.push(key),
            }
        }
        if !missing.is_empty() {
            return format!("Cannot save correction, missing fields: {}.", missing.join(", "));
        }
        let [original_text, corrected_text, explanation, improvement] = fields;

        let (Ok(user_id), Ok(conversation_id)) =
            (Uuid::parse_str(&ctx.user_id), Uuid::parse_str(&ctx.conversation_id))
        else {
            return "Failed to save correction: invalid ids.".to_string();
        };

        match self
            .apply(
                user_id,
                conversation_id,
                &original_text,
                &corrected_text,
                &explanation,
                &improvement,
            )
            .await
        {
            Ok(msg) => msg,
            Err(err) => {
                tracing::error!(%err, "save_correction failed");
                format!("Failed to save correction: {err}")
            }
        }
    }
}

impl SaveCorrectionAgent {
    #[allow(clippy::too_many_arguments)]
    async fn apply(
        &self,
        user_id: Uuid,
        conversation_id: Uuid,
        original_text: &str,
        corrected_text: &str,
        explanation: &str,
        improvement: &str,
    ) -> anyhow::Result<String> {
        let last_human: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM messages \
             WHERE conversation_id = $1 AND role = $2 \
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(conversation_id)
        .bind(MessageRole::Human)
        .fetch_optional(&self.db)
        .await?;

        let Some(message_id) = last_human else {
            return Ok("Error: no user message found to attach the correction.".to_string());
        };

        sqlx::query(
            "INSERT INTO grammar_corrections \
             (id, message_id, user_id, original_text, corrected_text, explanation, improvement) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (message_id) DO NOTHING",
        )
        .bind(Uuid::new_v4())
        .bind(message_id)
        .bind(user_id)
        .bind(original_text)
        .bind(corrected_text)
        .bind(explanation)
        .bind(improvement)
        .execute(&self.db)
        .await?;

        let payload = json!({
            "original_text": original_text,
            "corrected_text": corrected_text,
            "explanation": explanation,
            "improvement": improvement,
        });
        self.store
            .put_correction(&user_id.to_string(), &conversation_id.to_string(), &payload)
            .await
            .map_err(|e| anyhow::anyhow!("correction cache write failed: {e}"))?;

        Ok("Grammar correction saved successfully.".to_string())
    }
}
