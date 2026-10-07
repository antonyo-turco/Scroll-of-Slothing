//! Tauri commands. Each one is a one-line delegation to the service layer —
//! keep logic out of here so MCP/CLI get identical behaviour.

use serde::Serialize;
use serde_json::{Map, Value};
use sleuth_core::settings::Settings;
use sleuth_service::{JobStatus, Service, ServiceError, SettingsUpdate};
use tauri::State;

type Res<T> = Result<T, ServiceError>;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    data_dir: String,
}

#[tauri::command]
pub fn app_info(svc: State<'_, Service>) -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION"), data_dir: svc.data_dir().display().to_string() }
}

#[tauri::command]
pub async fn settings_get(svc: State<'_, Service>) -> Res<Settings> {
    svc.settings_get().await
}

#[tauri::command]
pub async fn settings_set(
    svc: State<'_, Service>,
    patch: Map<String, Value>,
) -> Res<SettingsUpdate> {
    svc.settings_set(patch).await
}

#[tauri::command]
pub async fn settings_reset(svc: State<'_, Service>) -> Res<Settings> {
    svc.settings_reset().await
}

#[tauri::command]
pub fn jobs_list(svc: State<'_, Service>) -> Vec<JobStatus> {
    svc.jobs_list()
}

#[tauri::command]
pub fn jobs_cancel(svc: State<'_, Service>, id: String) -> Res<()> {
    svc.job_cancel(&id)
}
