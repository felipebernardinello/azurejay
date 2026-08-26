use serde::Deserialize;

const GROQ_STT_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const DEFAULT_MODEL: &str = "whisper-large-v3-turbo";

#[derive(Debug, Deserialize)]
struct Transcription {
    #[serde(default)]
    text: String,
}

pub async fn transcribe(
    http: &reqwest::Client,
    api_key: &str,
    audio: Vec<u8>,
    filename: &str,
) -> anyhow::Result<Option<String>> {
    let part = reqwest::multipart::Part::bytes(audio)
        .file_name(filename.to_string())
        .mime_str("application/octet-stream")?;

    let form = reqwest::multipart::Form::new()
        .text("model", DEFAULT_MODEL)
        .text("language", "en")
        .text("temperature", "0.0")
        .text("response_format", "json")
        .part("file", part);

    let resp = http
        .post(GROQ_STT_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?
        .json::<Transcription>()
        .await?;

    let text = resp.text.trim().to_string();
    Ok((!text.is_empty()).then_some(text))
}
