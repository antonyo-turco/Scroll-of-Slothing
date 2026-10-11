//! Tauri shell: thin commands over `sleuth_service`, an event bridge, and the
//! `sleuth://` protocol that serves stored images without base64 over IPC.

mod commands;
mod mcp_http;
mod protocol;

use std::time::Duration;

use sleuth_service::Service;
use tauri::{Emitter, Manager};

/// Name of the Tauri event carrying [`sleuth_service::Event`]s to the UI.
pub const EVENT_CHANNEL: &str = "sleuth://event";
/// Tauri event sent after a `.dmix` opened with the app was imported.
pub const DMIX_OPENED_EVENT: &str = "music://dmix-opened";

/// Imports and starts every `.dmix` path in `args` (file association).
fn open_dmix_args(app: &tauri::AppHandle, args: Vec<String>) {
    let svc = app.state::<Service>().inner().clone();
    let app = app.clone();
    for path in args.into_iter().skip(1).filter(|a| a.to_ascii_lowercase().ends_with(".dmix")) {
        let (svc, app) = (svc.clone(), app.clone());
        tauri::async_runtime::spawn(async move {
            match svc.music_mix_import(path.clone().into(), true).await {
                Ok(r) => {
                    let payload =
                        serde_json::json!({ "soundscape_id": r.soundscape_id, "name": r.name });
                    let _ = app.emit(DMIX_OPENED_EVENT, payload);
                }
                Err(e) => tracing::warn!(%e, path, "cannot open mix"),
            }
        });
    }
}

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
        // A second launch (e.g. double-clicking a .dmix) hands its args to us.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
            }
            open_dmix_args(app, args);
        }))
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("sleuth", protocol::handle)
        .manage(service)
        .manage(mcp_http::McpHttp::default())
        .setup(|app| {
            let svc = app.state::<Service>().inner().clone();
            let handle = app.handle().clone();
            svc.attach_ui();
            // This process hosts the audio engine; clear stale live state first.
            if let Err(e) = tauri::async_runtime::block_on(svc.music_attach_engine()) {
                tracing::warn!(%e, "music engine attach failed");
            }
            open_dmix_args(&handle, std::env::args().collect());
            tauri::async_runtime::spawn(async move {
                svc.spawn_change_watcher(Duration::from_millis(500));
                let mut rx = svc.subscribe();
                mcp_http::sync(&svc, &handle.state::<mcp_http::McpHttp>()).await;
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            if matches!(&ev, sleuth_service::Event::StoreChanged { entity, .. } if entity == "settings") {
                                mcp_http::sync(&svc, &handle.state::<mcp_http::McpHttp>()).await;
                            }
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
            mcp_http::mcp_http_status,
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::settings_reset,
            commands::workspace_get,
            commands::workspace_set,
            commands::jobs_list,
            commands::jobs_cancel,
            commands::legacy_import,
            commands::library_search,
            commands::library_get,
            commands::library_update,
            commands::library_set_tags,
            commands::library_delete,
            commands::library_import,
            commands::library_export,
            commands::library_export_many,
            commands::library_copy_to_clipboard,
            commands::library_bulk_tag,
            commands::library_bulk_pin,
            commands::tags_list,
            commands::tags_rename,
            commands::tags_merge,
            commands::tags_delete,
            commands::token_borders_list,
            commands::token_borders_add,
            commands::token_borders_remove,
            commands::image_source_from_path,
            commands::image_source_from_clipboard,
            commands::image_source_info,
            commands::token_render,
            commands::token_save_png,
            commands::token_save_to_library,
            commands::token_open_asset,
            commands::token_sheet_get,
            commands::token_sheet_add_assets,
            commands::token_sheet_add_files,
            commands::token_sheet_add_composed,
            commands::token_sheet_update,
            commands::token_sheet_remove,
            commands::token_sheet_clear,
            commands::token_sheet_set_options,
            commands::token_sheet_layout,
            commands::token_sheet_export_pdf,
            commands::cutout_render,
            commands::cutout_save_png,
            commands::cutout_save_to_library,
            commands::cutout_open_asset,
            commands::object_sheet_get,
            commands::object_sheet_add_assets,
            commands::object_sheet_add_files,
            commands::object_sheet_add_composed,
            commands::object_sheet_update,
            commands::object_sheet_remove,
            commands::object_sheet_clear,
            commands::object_sheet_set_options,
            commands::object_sheet_layout,
            commands::object_sheet_export_pdf,
            commands::cards_templates,
            commands::cards_layout,
            commands::cards_validate,
            commands::cards_export_pdf,
            commands::decks_list,
            commands::decks_get,
            commands::decks_save,
            commands::decks_delete,
            commands::deck_read_file,
            commands::deck_write_file,
            commands::map_get,
            commands::map_set,
            commands::map_open_file,
            commands::map_open_source,
            commands::map_open_asset,
            commands::map_preview,
            commands::map_layout,
            commands::map_preview_page,
            commands::map_dewarp,
            commands::map_undo_dewarp,
            commands::map_fewest_pages,
            commands::map_detect_grid,
            commands::map_fit_region,
            commands::map_export_pdf,
            commands::map_save_asset,
            commands::map_filter_catalog,
            commands::aoe_presets_list,
            commands::aoe_presets_save,
            commands::aoe_presets_delete,
            commands::aoe_render,
            commands::aoe_info,
            commands::aoe_export_png,
            commands::aoe_export_template_pdf,
            commands::aoe_sheet_get,
            commands::aoe_sheet_add,
            commands::aoe_sheet_remove,
            commands::aoe_sheet_clear,
            commands::aoe_sheet_set_gap,
            commands::aoe_sheet_layout,
            commands::aoe_sheet_preview,
            commands::aoe_sheet_export_pdf,
            commands::music_tracks_list,
            commands::music_track_get,
            commands::music_track_create,
            commands::music_track_update,
            commands::music_track_delete,
            commands::music_fetch_youtube_meta,
            commands::music_probe_file,
            commands::music_state,
            commands::music_channel_add,
            commands::music_channel_update,
            commands::music_channel_remove,
            commands::music_set_all_playing,
            commands::music_stop_all,
            commands::music_soundbite_add,
            commands::music_soundbite_update,
            commands::music_soundbite_remove,
            commands::music_soundbite_play,
            commands::music_set_duck,
            commands::music_set_master,
            commands::music_preview,
            commands::music_stop_preview,
            commands::music_toggle_video,
            commands::music_engine_event,
            commands::music_sync,
            commands::music_soundscapes_list,
            commands::music_soundscape_save,
            commands::music_soundscape_update,
            commands::music_soundscape_load,
            commands::music_soundscape_delete,
            commands::music_playlists_list,
            commands::music_playlist_save,
            commands::music_playlist_delete,
            commands::music_playlist_play,
            commands::music_playlist_control,
            commands::music_soundboards_list,
            commands::music_soundboard_save,
            commands::music_soundboard_delete,
            commands::music_soundboard_press,
            commands::music_mix_export,
            commands::music_mix_import,
            commands::music_import_legacy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
