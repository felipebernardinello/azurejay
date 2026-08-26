use chrono::{DateTime, Utc};
use crate::coa::HistoryTurn;
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::entities::{Conversation, GrammarCorrection, Message, MessageRole};
use crate::error::AppError;
use super::language::{NON_ENGLISH_REPLY, is_confidently_non_english};
use crate::state::AppState;


#[derive(Debug, Serialize)]
pub struct ConversationListItem {
    pub id: Uuid,
    pub title: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct GrammarCorrectionDetail {
    pub original_text: String,
    pub corrected_text: String,
    pub explanation: String,
    pub improvement: String,
}

#[derive(Debug, Serialize)]
pub struct MessageDetail {
    pub id: Uuid,
    pub role: MessageRole,
    pub content: String,
    pub correction: Option<GrammarCorrectionDetail>,
}

#[derive(Debug, Serialize)]
pub struct ConversationHistoryResponse {
    pub id: Uuid,
    pub title: String,
    pub messages: Vec<MessageDetail>,
}

#[derive(Debug, Serialize)]
pub struct NewConversationResponse {
    pub response: String,
    pub conversation_id: Uuid,
    pub title: String,
}


pub async fn list_conversations(
    db: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ConversationListItem>, AppError> {
    let rows = sqlx::query_as::<_, Conversation>(
        "SELECT * FROM conversations WHERE user_id = $1 ORDER BY updated_at DESC",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|c| ConversationListItem { id: c.id, title: c.title, updated_at: c.updated_at })
        .collect())
}

async fn owned_conversation(
    db: &PgPool,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<Conversation, AppError> {
    let conv = sqlx::query_as::<_, Conversation>("SELECT * FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)?;

    if conv.user_id != user_id {
        return Err(AppError::Forbidden);
    }
    Ok(conv)
}

pub async fn get_conversation_history(
    db: &PgPool,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<ConversationHistoryResponse, AppError> {
    let conv = owned_conversation(db, user_id, conversation_id).await?;

    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(db)
    .await?;

    let corrections = sqlx::query_as::<_, GrammarCorrection>(
        "SELECT gc.* FROM grammar_corrections gc \
         JOIN messages m ON m.id = gc.message_id \
         WHERE m.conversation_id = $1",
    )
    .bind(conversation_id)
    .fetch_all(db)
    .await?;

    let details = messages
        .into_iter()
        .map(|m| {
            let correction = corrections
                .iter()
                .find(|c| c.message_id == m.id)
                .map(|c| GrammarCorrectionDetail {
                    original_text: c.original_text.clone(),
                    corrected_text: c.corrected_text.clone(),
                    explanation: c.explanation.clone(),
                    improvement: c.improvement.clone(),
                });
            MessageDetail { id: m.id, role: m.role, content: m.content, correction }
        })
        .collect();

    Ok(ConversationHistoryResponse { id: conv.id, title: conv.title, messages: details })
}


async fn insert_message(
    db: &PgPool,
    conversation_id: Uuid,
    role: MessageRole,
    content: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO messages (id, conversation_id, role, content) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn process_new_message(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
    user_input: &str,
) -> Result<String, AppError> {
    if is_confidently_non_english(user_input) {
        insert_message(&state.db, conversation_id, MessageRole::Human, user_input).await?;
        insert_message(&state.db, conversation_id, MessageRole::Ai, NON_ENGLISH_REPLY).await?;
        touch_conversation(&state.db, conversation_id).await?;
        return Ok(NON_ENGLISH_REPLY.to_string());
    }

    insert_message(&state.db, conversation_id, MessageRole::Human, user_input).await?;

    let history = load_history_turns(&state.db, conversation_id).await?;

    let user_profile = match state.store.get_profile(&user_id.to_string()).await {
        Ok(Some(value)) => value.to_string(),
        Ok(None) => "No profile saved yet.".to_string(),
        Err(err) => return Err(AppError::Other(anyhow::anyhow!("profile store read failed: {err}"))),
    };

    let result = state
        .coa
        .run(
            user_input,
            &history,
            &user_profile,
            &user_id.to_string(),
            &conversation_id.to_string(),
        )
        .await?;

    let answer = result.answer;
    insert_message(&state.db, conversation_id, MessageRole::Ai, &answer).await?;
    touch_conversation(&state.db, conversation_id).await?;

    tracing::info!(
        %conversation_id,
        steps = result.steps,
        replans = result.replans,
        "CoA episode complete"
    );
    Ok(answer)
}

pub async fn create_new_conversation(
    state: &AppState,
    user_id: Uuid,
    content: &str,
) -> Result<NewConversationResponse, AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::bad_request("message content cannot be empty"));
    }
    let title = make_title(content);

    let conversation_id = Uuid::new_v4();
    sqlx::query("INSERT INTO conversations (id, user_id, title) VALUES ($1, $2, $3)")
        .bind(conversation_id)
        .bind(user_id)
        .bind(&title)
        .execute(&state.db)
        .await?;

    let response = process_new_message(state, user_id, conversation_id, content).await?;
    Ok(NewConversationResponse { response, conversation_id, title })
}

pub async fn chat(
    state: &AppState,
    user_id: Uuid,
    conversation_id: Uuid,
    content: &str,
) -> Result<String, AppError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(AppError::bad_request("message content cannot be empty"));
    }
    owned_conversation(&state.db, user_id, conversation_id).await?;
    process_new_message(state, user_id, conversation_id, content).await
}

pub async fn delete_conversation(
    db: &PgPool,
    user_id: Uuid,
    conversation_id: Uuid,
) -> Result<(), AppError> {
    owned_conversation(db, user_id, conversation_id).await?;
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(db)
        .await?;
    Ok(())
}


async fn load_history_turns(
    db: &PgPool,
    conversation_id: Uuid,
) -> Result<Vec<HistoryTurn>, AppError> {
    let messages = sqlx::query_as::<_, Message>(
        "SELECT * FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(db)
    .await?;

    Ok(messages
        .into_iter()
        .map(|m| HistoryTurn { role: m.role.as_str().to_string(), content: m.content })
        .collect())
}

async fn touch_conversation(db: &PgPool, conversation_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
        .bind(conversation_id)
        .execute(db)
        .await?;
    Ok(())
}

fn make_title(content: &str) -> String {
    let trimmed: String = content.chars().take(60).collect();
    if content.chars().count() > 60 {
        format!("{}...", trimmed.trim_end())
    } else {
        trimmed.trim().to_string()
    }
}
