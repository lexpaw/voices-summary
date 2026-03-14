use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use directories::ProjectDirs;
use tokio::sync::RwLock;

use crate::{
    errors::{AppError, AppResult},
    models::AppSettings,
};

#[derive(Clone)]
pub struct ConfigStore {
    path: Arc<PathBuf>,
    settings: Arc<RwLock<AppSettings>>,
}

impl ConfigStore {
    pub async fn new() -> AppResult<Self> {
        let project_dirs = ProjectDirs::from("com", "openai", "voices-summary")
            .ok_or_else(|| AppError::Config("无法定位应用配置目录".to_string()))?;
        let config_dir = project_dirs.config_dir();
        tokio::fs::create_dir_all(config_dir).await?;
        let path = config_dir.join("settings.json");

        let settings = if path.exists() {
            let content = tokio::fs::read_to_string(&path).await?;
            serde_json::from_str::<AppSettings>(&content)?
        } else {
            let default = AppSettings::default();
            let serialized = serde_json::to_string_pretty(&default)?;
            tokio::fs::write(&path, serialized).await?;
            default
        };

        Ok(Self {
            path: Arc::new(path),
            settings: Arc::new(RwLock::new(settings)),
        })
    }

    pub async fn get(&self) -> AppSettings {
        self.settings.read().await.clone()
    }

    pub fn blocking_get(&self) -> AppSettings {
        self.settings.blocking_read().clone()
    }

    pub async fn save(&self, settings: AppSettings) -> AppResult<AppSettings> {
        validate_settings(&settings)?;
        ensure_directories(&settings).await?;
        let serialized = serde_json::to_string_pretty(&settings)?;
        tokio::fs::write(&*self.path, serialized).await?;
        *self.settings.write().await = settings.clone();
        Ok(settings)
    }

    pub fn file_path(&self) -> &Path {
        self.path.as_path()
    }
}

pub async fn ensure_directories(settings: &AppSettings) -> AppResult<()> {
    tokio::fs::create_dir_all(Path::new(&settings.data_dir).join("raw")).await?;
    tokio::fs::create_dir_all(Path::new(&settings.output_dir).join("transcripts")).await?;
    tokio::fs::create_dir_all(Path::new(&settings.output_dir).join("summaries")).await?;
    Ok(())
}

pub fn validate_settings(settings: &AppSettings) -> AppResult<()> {
    if settings.data_dir.trim().is_empty() {
        return Err(AppError::Config("数据目录不能为空".to_string()));
    }
    if settings.output_dir.trim().is_empty() {
        return Err(AppError::Config("输出目录不能为空".to_string()));
    }
    if settings.transcription_provider.base_url.trim().is_empty() {
        return Err(AppError::Config("转写服务 URL 不能为空".to_string()));
    }
    if settings.summary_provider.base_url.trim().is_empty() {
        return Err(AppError::Config("摘要服务 URL 不能为空".to_string()));
    }
    if settings.processing_concurrency == 0 {
        return Err(AppError::Config("并发任务数至少为 1".to_string()));
    }
    Ok(())
}
