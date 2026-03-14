use std::{collections::HashSet, path::PathBuf, sync::Arc};

use tokio::sync::{Mutex, RwLock};

use crate::{
    config::ConfigStore,
    db::Database,
    errors::{AppError, AppResult},
    models::{AppSettings, AppStatus, AudioRecord, AudioRecordDetail, AudioRecordQuery, DetectedDevice, LogEntry},
    services::{ingestion, jobs},
};

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: ConfigStore,
    pub connected_devices: Arc<RwLock<Vec<DetectedDevice>>>,
    pub active_jobs: Arc<Mutex<HashSet<String>>>,
}

impl AppState {
    pub fn new(db: Database, config: ConfigStore) -> Self {
        Self {
            db,
            config,
            connected_devices: Arc::new(RwLock::new(Vec::new())),
            active_jobs: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

#[tauri::command]
pub async fn get_app_status(state: tauri::State<'_, Arc<AppState>>) -> Result<AppStatus, String> {
    let devices = state.connected_devices.read().await.clone();
    state.db.build_status(devices).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_audio_records(
    state: tauri::State<'_, Arc<AppState>>,
    query: AudioRecordQuery,
) -> Result<Vec<AudioRecord>, String> {
    state
        .db
        .list_audio_records(&query)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_audio_record_detail(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<AudioRecordDetail, String> {
    state
        .db
        .get_audio_record_detail(&id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "找不到音频记录".to_string())
}

#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    Ok(state.config.get().await)
}

#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    let saved = state
        .config
        .save(settings)
        .await
        .map_err(|error| error.to_string())?;
    state
        .db
        .log("INFO", "settings", "应用设置已更新")
        .map_err(|error| error.to_string())?;
    Ok(saved)
}

#[tauri::command]
pub async fn trigger_scan(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    ingestion::scan_and_import(state.inner().clone())
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn retry_job(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    stage: String,
) -> Result<(), String> {
    jobs::retry_job(state.inner().clone(), &id, &stage)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_logs(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<LogEntry>, String> {
    state.db.list_logs(200).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_path(
    state: tauri::State<'_, Arc<AppState>>,
    kind: String,
    id: Option<String>,
) -> Result<(), String> {
    let path = resolve_open_path(state.inner(), &kind, id.as_deref()).map_err(|error| error.to_string())?;
    open_with_shell(path).map_err(|error| error.to_string())
}

fn resolve_open_path(state: &Arc<AppState>, kind: &str, id: Option<&str>) -> AppResult<PathBuf> {
    match kind {
        "data_dir" => Ok(PathBuf::from(state.config.blocking_get().data_dir)),
        "output_dir" => Ok(PathBuf::from(state.config.blocking_get().output_dir)),
        "audio" | "transcript" | "summary" => {
            let id = id.ok_or_else(|| AppError::Path("缺少音频 ID".to_string()))?;
            let record = state
                .db
                .get_record_by_id(id)?
                .ok_or_else(|| AppError::Path("找不到音频记录".to_string()))?;
            let path = match kind {
                "audio" => record.imported_path,
                "transcript" => record
                    .transcript_path
                    .ok_or_else(|| AppError::Path("转写稿尚未生成".to_string()))?,
                _ => record
                    .summary_path
                    .ok_or_else(|| AppError::Path("摘要尚未生成".to_string()))?,
            };
            Ok(PathBuf::from(path))
        }
        _ => Err(AppError::Config("未知路径类型".to_string())),
    }
}

#[cfg(target_os = "windows")]
fn open_with_shell(path: PathBuf) -> AppResult<()> {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(AppError::from)
}

#[cfg(not(target_os = "windows"))]
fn open_with_shell(_path: PathBuf) -> AppResult<()> {
    Err(AppError::UnsupportedPlatform)
}
