use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use super::service::{change_password, get_user};
use crate::auth::AuthUser;
use crate::entities::User;
use crate::error::AppError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(me))
        .route("/me/profile", get(profile))
        .route("/change-password", put(change_password_handler))
}

async fn me(State(state): State<AppState>, user: AuthUser) -> Result<Json<User>, AppError> {
    Ok(Json(get_user(&state.db, user.user_id).await?))
}

#[derive(Debug, Serialize)]
struct ProfileResponse {
    first_name: String,
    last_name: String,
    location: Option<String>,
    interests: Vec<String>,
}

async fn profile(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<ProfileResponse>, AppError> {
    let u = get_user(&state.db, user.user_id).await?;
    Ok(Json(ProfileResponse {
        first_name: u.first_name,
        last_name: u.last_name,
        location: u.location,
        interests: u.user_interests.unwrap_or_default(),
    }))
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password_handler(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<StatusCode, AppError> {
    change_password(&state.db, user, &body.current_password, &body.new_password).await?;
    Ok(StatusCode::NO_CONTENT)
}
