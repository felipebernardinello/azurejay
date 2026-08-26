use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::service;
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(history).delete(delete))
        .route("/{id}/chat", post(chat))
}

#[derive(Debug, Deserialize)]
struct NewConversationRequest {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatRequest {
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    response: String,
}

async fn list(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<service::ConversationListItem>>, AppError> {
    Ok(Json(service::list_conversations(&state.db, user.user_id).await?))
}

async fn history(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<service::ConversationHistoryResponse>, AppError> {
    Ok(Json(service::get_conversation_history(&state.db, user.user_id, id).await?))
}

async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<NewConversationRequest>,
) -> Result<(StatusCode, Json<service::NewConversationResponse>), AppError> {
    let resp = service::create_new_conversation(&state, user.user_id, &body.content).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn chat(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    let response = service::chat(&state, user.user_id, id, &body.content).await?;
    Ok(Json(ChatResponse { response }))
}

async fn delete(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    service::delete_conversation(&state.db, user.user_id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
