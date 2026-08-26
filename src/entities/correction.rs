use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct GrammarCorrection {
    pub id: Uuid,
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub original_text: String,
    pub corrected_text: String,
    pub explanation: String,
    pub improvement: String,
}
