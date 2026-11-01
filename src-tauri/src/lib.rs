mod archive;
mod commands;
mod database;
mod downloader;
mod error;
mod fonts;
mod installer;
mod metadata;
mod security;
mod state;

use database::Db;
use downloader::DownloadManager;
use installer::Installer;
use state::AppState;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::Manager;

const SYNC_INTERVAL_DAYS: i64 = 7;

static STATE: OnceLock<Arc<AppState>> = OnceLock::new();

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data_dir = app.path().app_data_dir()?;
    let cache_dir = app.path().app_cache_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&cache_dir)?;

    let db = Db::open(&data_dir.join("typevault.sqlite"))?;
    let http = state::http_client()?;
    let downloads = DownloadManager::new(db.clone(), http.clone(), handle.clone());
    let root = installer::user_font_root()?;
    let installer = Installer::new(db.clone(), handle.clone(), root);
    let state = Arc::new(AppState {
        db,
        http,
        downloads: downloads.clone(),
        installer: installer.clone(),
        syncing: AtomicBool::new(false),
        default_font_dir: data_dir.join("Fonts"),
        cache_dir,
    });
    downloads.attach(&state);
    downloads.start()?;
    app.manage(state.clone());
    let _ = STATE.set(state.clone());

    // Yavaş işler pencere açılışını bekletmez.
    let background = state.clone();
    tauri::async_runtime::spawn(async move {
        let worker = background.clone();
        let _ = tokio::task::spawn_blocking(move || worker.installer.reconcile()).await;
        tokio::time::sleep(Duration::from_millis(400)).await;
        let last = background.db.setting("last_sync").ok().flatten();
        let stale = last
            .and_then(|l| chrono::DateTime::parse_from_rfc3339(&l).ok())
            .is_none_or(|t| (chrono::Utc::now() - t.to_utc()).num_days() >= SYNC_INTERVAL_DAYS);
        let auto = background.db.setting_or("auto_sync", "1") == "1";
        let empty = background
            .db
            .conn()
            .and_then(|c| Ok(c.query_row("SELECT count(*)=0 FROM fonts", [], |r| r.get::<_, bool>(0))?))
            .unwrap_or(true);
        if empty || (auto && stale) {
            metadata::sync::spawn_sync(handle, background);
        }
    });
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol("tvfont", |_ctx, request, responder| {
            tauri::async_runtime::spawn(async move {
                let response = match STATE.get() {
                    Some(state) => fonts::preview::handle(state.clone(), request).await,
                    None => tauri::http::Response::builder().status(503).body(Vec::new()).expect("yanıt"),
                };
                responder.respond(response);
            });
        })
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::catalog::list_fonts,
            commands::catalog::get_font,
            commands::catalog::catalog_status,
            commands::catalog::sync_catalog,
            commands::catalog::test_api_key,
            commands::collections::list_collections,
            commands::collections::create_collection,
            commands::collections::rename_collection,
            commands::collections::delete_collection,
            commands::collections::add_to_collection,
            commands::collections::remove_from_collection,
            commands::collections::toggle_favorite,
            commands::transfer::download_font,
            commands::transfer::install_font,
            commands::transfer::uninstall_font,
            commands::transfer::delete_downloaded,
            commands::transfer::start_bulk,
            commands::transfer::list_batches,
            commands::transfer::dismiss_batch,
            commands::transfer::retry_batch_failures,
            commands::transfer::list_downloads,
            commands::transfer::download_snapshot,
            commands::transfer::pause_download,
            commands::transfer::resume_download,
            commands::transfer::cancel_download,
            commands::transfer::pause_all,
            commands::transfer::resume_all,
            commands::transfer::clear_finished_downloads,
            commands::transfer::install_all_downloaded,
            commands::library::library_stats,
            commands::library::scan_system_fonts,
            commands::library::list_system_fonts,
            commands::library::get_settings,
            commands::library::set_setting,
            commands::library::choose_download_dir,
            commands::library::reset_download_dir,
            commands::library::storage_info,
            commands::library::clear_cache,
            commands::library::export_library,
            commands::library::reveal_installed_folder,
            commands::library::reveal_download_folder,
        ])
        .run(tauri::generate_context!())
        .expect("TYPEVAULT başlatılamadı");
}
