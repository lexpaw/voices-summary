#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{path::PathBuf, sync::Arc};

use directories::ProjectDirs;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use voices_summary_lib::{
    commands::AppState,
    config::ConfigStore,
    db::Database,
    errors::{AppError, AppResult},
    models::{AppSettings, AppStatus, AudioRecord, AudioRecordDetail, AudioRecordQuery, LogEntry},
    services::{ingestion, jobs},
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("application startup failed: {error}");
    }
}

async fn run() -> AppResult<()> {
    let config = ConfigStore::new().await?;
    let database = Database::new(resolve_database_path()?)?;
    let state = Arc::new(AppState::new(database, config.clone()));

    tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            list_audio_records,
            get_audio_record_detail,
            get_settings,
            save_settings,
            trigger_scan,
            retry_job,
            list_logs,
            open_path
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            build_tray(app, state.clone())?;

            let watcher_state = state.clone();
            tauri::async_runtime::spawn(async move {
                ingestion::start_watch_loop(watcher_state).await;
            });

            let worker_state = state.clone();
            tauri::async_runtime::spawn(async move {
                jobs::start_worker_loop(worker_state).await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .map_err(|error| AppError::System(error.to_string()))
}

fn build_tray<R: tauri::Runtime>(app: &mut tauri::App<R>, state: Arc<AppState>) -> Result<(), tauri::Error> {
    let open = MenuItem::with_id(app, "open", "打开控制台", true, None::<&str>)?;
    let scan = MenuItem::with_id(app, "scan", "立即扫描设备", true, None::<&str>)?;
    let open_data = MenuItem::with_id(app, "open_data", "打开数据目录", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &scan, &open_data, &quit])?;

    let handle = app.handle().clone();

    TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "scan" => {
                let state = state.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = ingestion::scan_and_import(state).await;
                });
            }
            "open_data" => {
                let data_dir = state.config.blocking_get().data_dir;
                let _ = std::process::Command::new("explorer").arg(data_dir).spawn();
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

fn resolve_database_path() -> AppResult<PathBuf> {
    let dirs = ProjectDirs::from("com", "openai", "voices-summary")
        .ok_or_else(|| AppError::Config("无法定位应用数据目录".to_string()))?;
    std::fs::create_dir_all(dirs.data_local_dir())?;
    Ok(dirs.data_local_dir().join("voices-summary.db"))
}

#[tauri::command]
async fn get_app_status(state: tauri::State<'_, Arc<AppState>>) -> Result<AppStatus, String> {
    voices_summary_lib::commands::get_app_status(state).await
}

#[tauri::command]
fn list_audio_records(
    state: tauri::State<'_, Arc<AppState>>,
    query: AudioRecordQuery,
) -> Result<Vec<AudioRecord>, String> {
    voices_summary_lib::commands::list_audio_records(state, query)
}

#[tauri::command]
fn get_audio_record_detail(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<AudioRecordDetail, String> {
    voices_summary_lib::commands::get_audio_record_detail(state, id)
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, Arc<AppState>>) -> Result<AppSettings, String> {
    voices_summary_lib::commands::get_settings(state).await
}

#[tauri::command]
async fn save_settings(
    state: tauri::State<'_, Arc<AppState>>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    voices_summary_lib::commands::save_settings(state, settings).await
}

#[tauri::command]
async fn trigger_scan(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    voices_summary_lib::commands::trigger_scan(state).await
}

#[tauri::command]
async fn retry_job(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    stage: String,
) -> Result<(), String> {
    voices_summary_lib::commands::retry_job(state, id, stage).await
}

#[tauri::command]
fn list_logs(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<LogEntry>, String> {
    voices_summary_lib::commands::list_logs(state)
}

#[tauri::command]
fn open_path(
    state: tauri::State<'_, Arc<AppState>>,
    kind: String,
    id: Option<String>,
) -> Result<(), String> {
    voices_summary_lib::commands::open_path(state, kind, id)
}
