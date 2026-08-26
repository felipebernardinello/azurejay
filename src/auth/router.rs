use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Form, Json, Router, middleware};
use serde::{Deserialize, Serialize};

use super::service::{authenticate_user, create_access_token, register_user};
use crate::error::AppError;
use super::rate_limit::rate_limit_register;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub first_name: String,
    pub last_name: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
}

pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/",
            post(register).route_layer(middleware::from_fn_with_state(
                state.clone(),
                rate_limit_register,
            )),
        )
        .route("/token", post(login))
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<StatusCode, AppError> {
    validate_registration(&body)?;
    register_user(
        &state.db,
        body.email.trim(),
        body.first_name.trim(),
        body.last_name.trim(),
        &body.password,
    )
    .await?;
    Ok(StatusCode::CREATED)
}

async fn login(
    State(state): State<AppState>,
    Form(form): Form<LoginForm>,
) -> Result<Json<TokenResponse>, AppError> {
    let user = authenticate_user(&state.db, form.username.trim(), &form.password)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let token = create_access_token(
        &user.email,
        user.id,
        &state.config.jwt_secret,
        Duration::from_secs(state.config.access_token_expiry.as_secs()),
    )?;

    Ok(Json(TokenResponse { access_token: token, token_type: "bearer" }))
}

fn validate_registration(body: &RegisterRequest) -> Result<(), AppError> {
    if !body.email.contains('@') || body.email.len() < 3 {
        return Err(AppError::bad_request("a valid email is required"));
    }
    if body.password.len() < 8 {
        return Err(AppError::bad_request("password must be at least 8 characters"));
    }
    if body.first_name.trim().is_empty() || body.last_name.trim().is_empty() {
        return Err(AppError::bad_request("first and last name are required"));
    }
    Ok(())
}
