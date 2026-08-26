use std::collections::BTreeSet;

use async_trait::async_trait;
use crate::coa::{ToolAgent, ToolContext};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use super::parse_json_body;
use crate::database::store::MemoryStore;

pub struct UpdateProfileAgent {
    db: PgPool,
    store: MemoryStore,
}

impl UpdateProfileAgent {
    #[must_use]
    pub fn new(db: PgPool, store: MemoryStore) -> Self {
        Self { db, store }
    }
}

#[derive(sqlx::FromRow)]
struct ProfileRow {
    first_name: String,
    location: Option<String>,
    user_interests: Option<Vec<String>>,
}

#[async_trait]
impl ToolAgent for UpdateProfileAgent {
    fn name(&self) -> &str {
        "update_profile"
    }

    async fn run(&self, body: &str, ctx: &ToolContext) -> String {
        let params = parse_json_body(body);
        let name = params.get("name").and_then(|v| v.as_str());
        let location = params.get("location").and_then(|v| v.as_str());
        let interests_to_add: Vec<String> = params
            .get("interests_to_add")
            .or_else(|| params.get("interests"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let Ok(user_id) = Uuid::parse_str(&ctx.user_id) else {
            return "Failed to update profile: invalid user id.".to_string();
        };

        match self.apply(user_id, name, location, &interests_to_add).await {
            Ok(msg) => msg,
            Err(err) => {
                tracing::error!(%err, "update_profile failed");
                format!("Failed to update profile: {err}")
            }
        }
    }
}

impl UpdateProfileAgent {
    async fn apply(
        &self,
        user_id: Uuid,
        name: Option<&str>,
        location: Option<&str>,
        interests_to_add: &[String],
    ) -> anyhow::Result<String> {
        let Some(current) = sqlx::query_as::<_, ProfileRow>(
            "SELECT first_name, location, user_interests FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&self.db)
        .await?
        else {
            return Ok(format!("Error: user {user_id} not found."));
        };

        let first_name = name.unwrap_or(&current.first_name).to_string();
        let new_location = location.map(str::to_string).or(current.location);

        let mut interests: BTreeSet<String> =
            current.user_interests.unwrap_or_default().into_iter().collect();
        interests.extend(interests_to_add.iter().cloned());
        let interests: Vec<String> = interests.into_iter().collect();

        sqlx::query(
            "UPDATE users SET first_name = $1, location = $2, user_interests = $3 WHERE id = $4",
        )
        .bind(&first_name)
        .bind(&new_location)
        .bind(&interests)
        .bind(user_id)
        .execute(&self.db)
        .await?;

        let profile = json!({
            "name": first_name,
            "location": new_location,
            "interests": interests,
        });
        self.store
            .put_profile(&user_id.to_string(), &profile)
            .await
            .map_err(|e| anyhow::anyhow!("profile cache write failed: {e}"))?;

        Ok("User profile updated successfully.".to_string())
    }
}
