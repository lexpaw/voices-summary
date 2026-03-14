use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    commands::AppState,
    config::ensure_directories,
    device::discover_target_devices,
    errors::AppResult,
    models::{DetectedDevice, NewAudioRecord},
};

pub async fn start_watch_loop(state: Arc<AppState>) {
    if let Err(error) = scan_and_import(state.clone()).await {
        let _ = state
            .db
            .log("ERROR", "device", &format!("首次设备扫描失败: {error}"));
    }

    loop {
        let interval = state.config.get().await.scan_interval_secs.max(5);
        tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
        if let Err(error) = scan_and_import(state.clone()).await {
            let _ = state
                .db
                .log("ERROR", "device", &format!("周期设备扫描失败: {error}"));
        }
    }
}

pub async fn scan_and_import(state: Arc<AppState>) -> AppResult<usize> {
    let settings = state.config.get().await;
    ensure_directories(&settings).await?;

    let devices = discover_target_devices(&settings).await?;
    for device in &devices {
        state.db.upsert_device(device)?;
    }
    state.db.set_last_scan_at(Utc::now())?;
    {
        let mut lock = state.connected_devices.write().await;
        *lock = devices.clone();
    }

    let mut imported = 0_usize;
    for device in devices {
        if !device.path_hints_matched && !settings.device_match_rule.path_hints.is_empty() {
            continue;
        }
        imported += import_device_files(&state, &device).await?;
    }
    state
        .db
        .log("INFO", "device", &format!("设备扫描完成，本次新导入 {imported} 个音频文件"))?;
    Ok(imported)
}

async fn import_device_files(state: &Arc<AppState>, device: &DetectedDevice) -> AppResult<usize> {
    let settings = state.config.get().await;
    let roots = if settings.scan_directories.is_empty() {
        vec![PathBuf::from(format!("{}\\", device.drive_letter))]
    } else {
        settings
            .scan_directories
            .iter()
            .map(|dir| PathBuf::from(format!("{}\\{}", device.drive_letter, dir)))
            .filter(|path| path.exists())
            .collect::<Vec<_>>()
    };

    let mut imported_count = 0;
    for root in roots {
        for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if !settings.allowed_extensions.iter().any(|item| item.eq_ignore_ascii_case(&extension)) {
                continue;
            }

            let metadata = tokio::fs::metadata(path).await?;
            let modified = metadata.modified()?.elapsed().map(|value| value.as_secs()).unwrap_or_default();
            let relative_path = path
                .strip_prefix(format!("{}\\", device.drive_letter))
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let fingerprint = compute_fingerprint(&device.identifier, &relative_path, metadata.len(), modified);
            if state.db.has_fingerprint(&fingerprint)? {
                continue;
            }

            let audio_id = uuid::Uuid::new_v4().to_string();
            let imported_path = Path::new(&settings.data_dir)
                .join("raw")
                .join(format!("{audio_id}.{}", extension));
            tokio::fs::copy(path, &imported_path).await?;

            let record = NewAudioRecord {
                id: audio_id,
                device_identifier: device.identifier.clone(),
                file_name: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unknown")
                    .to_string(),
                relative_path,
                imported_path: imported_path.to_string_lossy().to_string(),
                fingerprint,
                synced_at: Utc::now(),
            };
            state.db.insert_audio_record(&record)?;
            imported_count += 1;
        }
    }

    Ok(imported_count)
}

fn compute_fingerprint(device_identifier: &str, relative_path: &str, size: u64, modified_hint: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(device_identifier.as_bytes());
    hasher.update(relative_path.as_bytes());
    hasher.update(size.to_le_bytes());
    hasher.update(modified_hint.to_le_bytes());
    format!("{:x}", hasher.finalize())
}
