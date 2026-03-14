use std::{path::Path, time::Duration};

use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    multipart,
    Client,
};
use serde::Deserialize;

use crate::{
    errors::{AppError, AppResult},
    models::{AppSettings, SummaryDocument, TranscriptSegment},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptionResponse {
    #[serde(default, alias = "transcriptSegments")]
    segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummaryResponse {
    title: Option<String>,
    #[serde(default)]
    bullets: Vec<String>,
    #[serde(alias = "full_text")]
    full_text: String,
}

pub async fn transcribe_audio(settings: &AppSettings, audio_path: &Path) -> AppResult<Vec<TranscriptSegment>> {
    let client = build_client(settings)?;
    let audio_bytes = tokio::fs::read(audio_path).await?;
    let file_name = audio_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("audio.bin")
        .to_string();

    let response = execute_with_retry(
        settings,
        || async {
            let part = multipart::Part::bytes(audio_bytes.clone()).file_name(file_name.clone());
            let form = multipart::Form::new()
                .part("file", part)
                .text("diarization_required", "true");
            let request = client
                .post(&settings.transcription_provider.base_url)
                .headers(build_headers(&settings.transcription_provider)?)
                .multipart(form);
            let response = request.send().await?;
            if !response.status().is_success() {
                return Err(status_error("转写服务", response.status().as_u16(), response.text().await.unwrap_or_default()));
            }
            Ok(response.json::<TranscriptionResponse>().await?)
        },
        "转写服务",
    )
    .await?;

    if response.segments.is_empty() {
        return Err(AppError::Provider("转写服务未返回任何分段结果".to_string()));
    }
    if response
        .segments
        .iter()
        .any(|segment| segment.speaker_label.trim().is_empty())
    {
        return Err(AppError::Provider("转写结果缺少说话人标记".to_string()));
    }
    Ok(response.segments)
}

pub async fn summarize_transcript(
    settings: &AppSettings,
    file_name: &str,
    segments: &[TranscriptSegment],
) -> AppResult<SummaryDocument> {
    let client = build_client(settings)?;
    let transcript = segments
        .iter()
        .map(|segment| format!("{}: {}", segment.speaker_label, segment.text))
        .collect::<Vec<_>>()
        .join("\n");

    let payload = serde_json::json!({
        "fileName": file_name,
        "transcript": transcript,
        "segments": segments
    });

    let response = execute_with_retry(
        settings,
        || async {
            let response = client
                .post(&settings.summary_provider.base_url)
                .headers(build_headers(&settings.summary_provider)?)
                .json(&payload)
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(status_error("摘要服务", response.status().as_u16(), response.text().await.unwrap_or_default()));
            }
            Ok(response.json::<SummaryResponse>().await?)
        },
        "摘要服务",
    )
    .await?;

    Ok(SummaryDocument {
        title: response.title,
        bullets: response.bullets,
        full_text: response.full_text,
    })
}

fn build_client(settings: &AppSettings) -> AppResult<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(settings.request_timeout_secs))
        .build()?)
}

fn build_headers(provider: &crate::models::ProviderSettings) -> AppResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    if let Some(api_key) = provider.api_key.as_ref().filter(|value| !value.trim().is_empty()) {
        let header_name = provider.auth_header.as_deref().unwrap_or("Authorization");
        headers.insert(
            HeaderName::from_bytes(header_name.as_bytes())
                .map_err(|_| AppError::Config("无效的鉴权 header 名称".to_string()))?,
            HeaderValue::from_str(api_key)
                .map_err(|_| AppError::Config("无效的鉴权 header 值".to_string()))?,
        );
    }
    for entry in &provider.extra_headers {
        headers.insert(
            HeaderName::from_bytes(entry.key.as_bytes())
                .map_err(|_| AppError::Config(format!("无效的 header 名称: {}", entry.key)))?,
            HeaderValue::from_str(&entry.value)
                .map_err(|_| AppError::Config(format!("无效的 header 值: {}", entry.key)))?,
        );
    }
    Ok(headers)
}

fn status_error(scope: &str, status: u16, body: String) -> AppError {
    AppError::Provider(format!("{scope} 返回 HTTP {status}: {body}"))
}

async fn execute_with_retry<F, Fut, T>(settings: &AppSettings, mut make_request: F, scope: &str) -> AppResult<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = AppResult<T>>,
{
    let max_retries = settings.max_retries;
    let mut last_error = None;
    for attempt in 0..=max_retries {
        match make_request().await {
            Ok(value) => return Ok(value),
            Err(error) => {
                let retryable = matches!(error, AppError::Http(_))
                    || matches!(&error, AppError::Provider(message) if message.contains("HTTP 5"));
                if attempt >= max_retries || !retryable {
                    return Err(error);
                }
                last_error = Some(error);
                tokio::time::sleep(Duration::from_secs(2_u64.saturating_pow(attempt + 1))).await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::Provider(format!("{scope} 未知错误"))))
}
