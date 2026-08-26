use redis::AsyncCommands;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone)]
pub struct MemoryStore {
    conn: redis::aio::ConnectionManager,
}

impl MemoryStore {
    pub async fn connect(redis_url: &str) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let conn = redis::aio::ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }

    fn profile_key(user_id: &str) -> String {
        format!("profile:{user_id}")
    }

    pub async fn get_profile(&self, user_id: &str) -> Result<Option<Value>, redis::RedisError> {
        let mut conn = self.conn.clone();
        let raw: Option<String> = conn.get(Self::profile_key(user_id)).await?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub async fn put_profile(
        &self,
        user_id: &str,
        profile: &impl Serialize,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.conn.clone();
        let json = serde_json::to_string(profile).unwrap_or_else(|_| "{}".into());
        let _: () = conn.set(Self::profile_key(user_id), json).await?;
        Ok(())
    }

    pub async fn put_correction(
        &self,
        user_id: &str,
        conversation_id: &str,
        correction: &impl Serialize,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.conn.clone();
        let id = uuid::Uuid::new_v4();
        let key = format!("corrections:{user_id}:{conversation_id}:{id}");
        let json = serde_json::to_string(correction).unwrap_or_else(|_| "{}".into());
        let _: () = conn.set(key, json).await?;
        Ok(())
    }
}
