use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::entities::User;
use crate::error::AppError;

pub async fn get_user(db: &PgPool, id: Uuid) -> Result<User, AppError> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn change_password(
    db: &PgPool,
    user: AuthUser,
    current_password: &str,
    new_password: &str,
) -> Result<(), AppError> {
    if new_password.len() < 8 {
        return Err(AppError::bad_request("new password must be at least 8 characters"));
    }

    let record = get_user(db, user.user_id).await?;
    if !crate::auth::verify(current_password, &record.password_hash) {
        return Err(AppError::bad_request("current password is incorrect"));
    }

    let new_hash = crate::auth::hash(new_password)?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(user.user_id)
        .execute(db)
        .await?;
    Ok(())
}
