use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::password::{hash_password, verify_password};
use crate::entities::User;
use crate::error::AppError;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub id: String,
    pub exp: usize,
}

pub fn create_access_token(
    email: &str,
    user_id: Uuid,
    secret: &str,
    expiry: Duration,
) -> Result<String, AppError> {
    let exp = Utc::now()
        .checked_add_signed(chrono::Duration::from_std(expiry).unwrap_or_default())
        .map(|t| t.timestamp())
        .unwrap_or_default()
        .max(0) as usize;

    let claims = Claims { sub: email.to_string(), id: user_id.to_string(), exp };
    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| AppError::Other(anyhow::anyhow!("failed to sign token: {e}")))
}

pub fn verify_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|err| {
        tracing::warn!(%err, "token verification failed");
        AppError::Unauthorized
    })
}

pub async fn register_user(
    db: &PgPool,
    email: &str,
    first_name: &str,
    last_name: &str,
    password: &str,
) -> Result<(), AppError> {
    let password_hash = hash_password(password)?;
    let result = sqlx::query(
        "INSERT INTO users (id, email, first_name, last_name, password_hash) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(first_name)
    .bind(last_name)
    .bind(&password_hash)
    .execute(db)
    .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            Err(AppError::Conflict("email already registered".into()))
        }
        Err(err) => Err(AppError::Database(err)),
    }
}

pub async fn authenticate_user(
    db: &PgPool,
    email: &str,
    password: &str,
) -> Result<Option<User>, AppError> {
    let user: Option<User> =
        sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = $1")
            .bind(email)
            .fetch_optional(db)
            .await?;

    Ok(match user {
        Some(u) if verify_password(password, &u.password_hash) => Some(u),
        _ => {
            tracing::warn!(%email, "failed authentication attempt");
            None
        }
    })
}
