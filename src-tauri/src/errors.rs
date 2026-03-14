use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("配置错误: {0}")]
    Config(String),
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] io::Error),
    #[error("网络错误: {0}")]
    Http(#[from] reqwest::Error),
    #[error("序列化错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("路径错误: {0}")]
    Path(String),
    #[error("外部服务错误: {0}")]
    Provider(String),
    #[error("系统错误: {0}")]
    System(String),
    #[error("不支持当前平台")]
    UnsupportedPlatform,
}

pub type AppResult<T> = Result<T, AppError>;
