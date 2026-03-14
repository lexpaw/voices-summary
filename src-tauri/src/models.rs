use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeaderEntry {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSettings {
    pub base_url: String,
    pub api_key: Option<String>,
    pub auth_header: Option<String>,
    #[serde(default)]
    pub extra_headers: Vec<HeaderEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceMatchRule {
    pub vid: Option<String>,
    pub pid: Option<String>,
    pub volume_label: Option<String>,
    #[serde(default)]
    pub path_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub data_dir: String,
    pub output_dir: String,
    #[serde(default = "default_scan_dirs")]
    pub scan_directories: Vec<String>,
    #[serde(default = "default_extensions")]
    pub allowed_extensions: Vec<String>,
    #[serde(default)]
    pub device_match_rule: DeviceMatchRule,
    pub transcription_provider: ProviderSettings,
    pub summary_provider: ProviderSettings,
    #[serde(default = "default_scan_interval")]
    pub scan_interval_secs: u64,
    #[serde(default = "default_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_retries")]
    pub max_retries: u32,
    #[serde(default = "default_concurrency")]
    pub processing_concurrency: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            data_dir: "D:\\VoicesSummary\\data".to_string(),
            output_dir: "D:\\VoicesSummary\\output".to_string(),
            scan_directories: default_scan_dirs(),
            allowed_extensions: default_extensions(),
            device_match_rule: DeviceMatchRule::default(),
            transcription_provider: ProviderSettings::default(),
            summary_provider: ProviderSettings::default(),
            scan_interval_secs: default_scan_interval(),
            request_timeout_secs: default_timeout_secs(),
            max_retries: default_retries(),
            processing_concurrency: default_concurrency(),
        }
    }
}

fn default_scan_dirs() -> Vec<String> {
    vec!["Record".to_string()]
}

fn default_extensions() -> Vec<String> {
    vec!["wav".to_string(), "mp3".to_string(), "m4a".to_string(), "aac".to_string()]
}

fn default_scan_interval() -> u64 {
    15
}

fn default_timeout_secs() -> u64 {
    90
}

fn default_retries() -> u32 {
    3
}

fn default_concurrency() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub identifier: String,
    pub volume_label: Option<String>,
    pub drive_letter: String,
    pub pnp_device_id: Option<String>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailureItem {
    pub audio_id: String,
    pub file_name: String,
    pub status: AudioStatus,
    pub error_message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioStatus {
    Imported,
    Transcribing,
    Transcribed,
    Summarizing,
    Completed,
    FailedTranscription,
    FailedSummary,
}

impl AudioStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioStatus::Imported => "imported",
            AudioStatus::Transcribing => "transcribing",
            AudioStatus::Transcribed => "transcribed",
            AudioStatus::Summarizing => "summarizing",
            AudioStatus::Completed => "completed",
            AudioStatus::FailedTranscription => "failed_transcription",
            AudioStatus::FailedSummary => "failed_summary",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "transcribing" => Self::Transcribing,
            "transcribed" => Self::Transcribed,
            "summarizing" => Self::Summarizing,
            "completed" => Self::Completed,
            "failed_transcription" => Self::FailedTranscription,
            "failed_summary" => Self::FailedSummary,
            _ => Self::Imported,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub device_connected: bool,
    pub connected_devices: Vec<DeviceInfo>,
    pub last_scan_at: Option<DateTime<Utc>>,
    pub total_files: i64,
    pub completed_files: i64,
    pub in_flight_files: i64,
    pub failed_files: i64,
    pub recent_failures: Vec<FailureItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub speaker_label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDocument {
    pub title: Option<String>,
    #[serde(default)]
    pub bullets: Vec<String>,
    pub full_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecord {
    pub id: String,
    pub device_identifier: String,
    pub file_name: String,
    pub relative_path: String,
    pub imported_path: String,
    pub transcript_path: Option<String>,
    pub summary_path: Option<String>,
    pub status: AudioStatus,
    pub error_message: Option<String>,
    pub synced_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecordDetail {
    #[serde(flatten)]
    pub record: AudioRecord,
    #[serde(default)]
    pub transcript_segments: Vec<TranscriptSegment>,
    pub summary: Option<SummaryDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioRecordQuery {
    pub search: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: i64,
    pub level: String,
    pub scope: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedDevice {
    pub identifier: String,
    pub drive_letter: String,
    pub volume_label: Option<String>,
    pub pnp_device_id: Option<String>,
    pub path_hints_matched: bool,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewAudioRecord {
    pub id: String,
    pub device_identifier: String,
    pub file_name: String,
    pub relative_path: String,
    pub imported_path: String,
    pub fingerprint: String,
    pub synced_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct PendingAudioJob {
    pub id: String,
    pub imported_path: String,
    pub file_name: String,
    pub status: AudioStatus,
}
