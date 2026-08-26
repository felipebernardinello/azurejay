use serde_json::json;

fn voice_id(name: &str) -> &'static str {
    match name.to_lowercase().as_str() {
        "shaun" => "mTSvIrm2hmcnOvb21nW2",
        "antoni" => "ErXwobaYiN019PkySvjV",
        _ => "21m00Tcm4TlvDq8ikWAM", // rachel (default)
    }
}

pub async fn synthesize(
    http: &reqwest::Client,
    api_key: Option<&str>,
    message: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let Some(api_key) = api_key else {
        tracing::warn!("ElevenLabs API key not configured; skipping TTS");
        return Ok(None);
    };
    if message.trim().is_empty() {
        return Ok(None);
    }

    let endpoint =
        format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id("rachel"));
    let body = json!({
        "text": message,
        "voice_settings": { "stability": 0.6, "similarity_boost": 0.8 },
    });

    let resp = http
        .post(endpoint)
        .header("xi-api-key", api_key)
        .header("accept", "audio/mpeg")
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    Ok(Some(resp.bytes().await?.to_vec()))
}
