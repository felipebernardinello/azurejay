use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::Router;
use uuid::Uuid;

const MAX_AUDIO_BYTES: usize = 32 * 1024 * 1024;

use super::stt;
use crate::tts;
use crate::auth::AuthUser;
use crate::error::AppError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", post(new_audio_conversation))
        .route("/chat/{id}", post(continue_audio_conversation))
        .layer(DefaultBodyLimit::max(MAX_AUDIO_BYTES))
}

async fn read_upload(mut multipart: Multipart) -> Result<(Vec<u8>, String), AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::bad_request(format!("invalid multipart body: {e}")))?
    {
        if field.name() == Some("file") {
            let filename = field.file_name().unwrap_or("audio.m4a").to_string();
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::bad_request(format!("could not read upload: {e}")))?;
            if bytes.is_empty() {
                return Err(AppError::bad_request("uploaded audio is empty"));
            }
            return Ok((bytes.to_vec(), filename));
        }
    }
    Err(AppError::bad_request("missing `file` field in multipart upload"))
}

async fn transcribe_upload(state: &AppState, multipart: Multipart) -> Result<String, AppError> {
    let (audio, filename) = read_upload(multipart).await?;
    stt::transcribe(&state.http, &state.config.groq_api_key, audio, &filename)
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("transcription failed: {e}")))?
        .ok_or_else(|| {
            AppError::bad_request(
                "could not transcribe audio; please ensure it is clear English speech",
            )
        })
}

async fn speak(state: &AppState, message: &str) -> Result<Vec<u8>, AppError> {
    tts::synthesize(&state.http, state.config.eleven_labs_api_key.as_deref(), message)
        .await
        .map_err(|e| AppError::Other(anyhow::anyhow!("speech synthesis failed: {e}")))?
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("speech synthesis unavailable")))
}

fn audio_response(bytes: Vec<u8>, extra: &[(&'static str, String)]) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=response.mp3"),
    );
    let mut expose = String::from("Content-Disposition");
    for &(name, ref value) in extra {
        if let Ok(v) = HeaderValue::from_str(value)
            && let Ok(hn) = header::HeaderName::from_bytes(name.as_bytes())
        {
            headers.insert(hn, v);
            expose.push(',');
            expose.push_str(name);
        }
    }
    if let Ok(v) = HeaderValue::from_str(&expose) {
        headers.insert("Access-Control-Expose-Headers", v);
    }
    (headers, bytes).into_response()
}

async fn new_audio_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let text = transcribe_upload(&state, multipart).await?;
    let conversation =
        crate::conversations::create_new_conversation(&state, user.user_id, &text).await?;
    let audio = speak(&state, &conversation.response).await?;

    Ok((
        StatusCode::CREATED,
        audio_response(
            audio,
            &[
                ("X-Conversation-ID", conversation.conversation_id.to_string()),
                ("X-Conversation-Title", conversation.title),
            ],
        ),
    )
        .into_response())
}

async fn continue_audio_conversation(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    multipart: Multipart,
) -> Result<Response, AppError> {
    let text = transcribe_upload(&state, multipart).await?;
    let answer = crate::conversations::chat(&state, user.user_id, id, &text).await?;
    let audio = speak(&state, &answer).await?;
    Ok(audio_response(audio, &[]))
}
