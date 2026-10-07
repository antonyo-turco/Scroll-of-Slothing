//! Tauri shell: thin commands over `sleuth_service`, an event bridge, and the
//! `sleuth://` protocol that serves stored images without base64 over IPC.

mod commands;
mod protocol;

use std::time::Duration;

use sleuth_service::Service;
use tauri::{Emitter, Manager};

/// Name of the Tauri event carrying [`sleuth_service::Event`]s to the UI.
pub const EVENT_CHANNEL: &str = "sleuth://event";

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = sleuth_service::default_data_dir();
    let service = Service::open(&data_dir).unwrap_or_else(|e| {
        panic!("cannot open data dir {}: {e}", data_dir.display());
    });
    tracing::info!(dir = %data_dir.display(), "data dir opened");

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("sleuth", protocol::handle)
        .manage(service)
        .setup(|app| {
            let svc = app.state::<Service>().inner().clone();
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                svc.spawn_change_watcher(Duration::from_millis(500));
                let mut rx = svc.subscribe();
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            let _ = handle.emit(EVENT_CHANNEL, ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(n, "ui event bridge lagged");
                        }
                        Err(_) => break,
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::settings_reset,
            commands::jobs_list,
            commands::jobs_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
