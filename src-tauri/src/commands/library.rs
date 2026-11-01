use crate::error::{AppError, Result};
use crate::installer::{self, user_font_root};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type St<'a> = State<'a, Arc<AppState>>;

const SETTING_KEYS: &[&str] = &[
    "theme", "auto_sync", "download_concurrency", "preview_text", "turkish_priority", "font_format", "google_api_key",
];

const DEFAULTS: &[(&str, &str)] = &[
    ("theme", "dark"),
    ("auto_sync", "1"),
    ("download_concurrency", "4"),
    ("preview_text", "TYPEVAULT Aa"),
    ("turkish_priority", "1"),
    ("font_format", "static"),
    ("google_api_key", ""),
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    available: i64,
    downloaded: i64,
    installed: i64,
    failed: i64,
    updates: i64,
    installed_files: i64,
    local_files: i64,
    local_bytes: i64,
    favorites: i64,
    active_downloads: i64,
}

#[tauri::command]
pub async fn library_stats(state: St<'_>) -> Result<LibraryStats> {
    state
        .db
        .run(|conn| {
            Ok(conn.query_row(
                "SELECT
                   coalesce(sum(in_catalog=1 AND local_status='available'),0),
                   coalesce(sum(local_status='downloaded'),0),
                   coalesce(sum(local_status='installed'),0),
                   coalesce(sum(local_status='install_failed'),0),
                   coalesce(sum(update_available),0),
                   (SELECT count(*) FROM installations WHERE status='installed' AND owner='typevault'),
                   (SELECT count(*) FROM font_files WHERE local_path IS NOT NULL),
                   (SELECT coalesce(sum(size_bytes),0) FROM font_files WHERE local_path IS NOT NULL),
                   (SELECT count(*) FROM favorites),
                   (SELECT count(*) FROM downloads WHERE status IN ('queued','downloading'))
                 FROM fonts",
                [],
                |r| {
                    Ok(LibraryStats {
                        available: r.get(0)?,
                        downloaded: r.get(1)?,
                        installed: r.get(2)?,
                        failed: r.get(3)?,
                        updates: r.get(4)?,
                        installed_files: r.get(5)?,
                        local_files: r.get(6)?,
                        local_bytes: r.get(7)?,
                        favorites: r.get(8)?,
                        active_downloads: r.get(9)?,
                    })
                },
            )?)
        })
        .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFamily {
    family: String,
    styles: i64,
    managed: bool,
    sample_postscript: String,
}

#[tauri::command]
pub async fn scan_system_fonts(state: St<'_>) -> Result<usize> {
    let db = state.db.clone();
    tokio::task::spawn_blocking(move || installer::scan_system(&db, &user_font_root()?))
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

#[tauri::command]
pub async fn list_system_fonts(state: St<'_>, search: String) -> Result<Vec<SystemFamily>> {
    state
        .db
        .run(move |conn| {
            let pattern = format!("%{}%", search.replace('%', "").replace('_', ""));
            let mut stmt = conn.prepare(
                "SELECT family, count(*), max(managed), min(postscript_name) FROM system_fonts
                 WHERE family NOT LIKE '.%' AND family LIKE ?1 GROUP BY family ORDER BY max(managed) DESC, family LIMIT 2000",
            )?;
            let rows = stmt.query_map([pattern], |r| {
                Ok(SystemFamily { family: r.get(0)?, styles: r.get(1)?, managed: r.get(2)?, sample_postscript: r.get(3)? })
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
        .await
}

#[tauri::command]
pub async fn get_settings(state: St<'_>) -> Result<HashMap<String, String>> {
    let fallback = state.default_font_dir.clone();
    let root = state.font_dir();
    state
        .db
        .run(move |conn| {
            let mut out: HashMap<String, String> =
                DEFAULTS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
                let (k, v) = row?;
                if SETTING_KEYS.contains(&k.as_str()) {
                    out.insert(k, v);
                }
            }
            out.insert("download_dir".into(), root.to_string_lossy().to_string());
            out.insert("default_download_dir".into(), fallback.to_string_lossy().to_string());
            Ok(out)
        })
        .await
}

fn validate_setting(key: &str, value: &str) -> Result<()> {
    let ok = match key {
        "theme" => matches!(value, "dark" | "light" | "system"),
        "auto_sync" | "turkish_priority" => matches!(value, "0" | "1"),
        "download_concurrency" => value.parse::<u8>().is_ok_and(|n| (1..=12).contains(&n)),
        "preview_text" => value.chars().count() <= 120,
        "font_format" => matches!(value, "static" | "variable"),
        "google_api_key" => value.len() <= 128,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::Invalid(format!("Geçersiz ayar: {key}")))
    }
}

#[tauri::command]
pub async fn set_setting(state: St<'_>, key: String, value: String) -> Result<()> {
    validate_setting(&key, &value)?;
    state.db.run(move |conn| crate::database::set_setting(conn, &key, &value)).await?;
    state.downloads.notify();
    Ok(())
}

/// Klasörü kullanıcı yerel iletişim kutusunda seçer; ön yüz yol vermez.
#[tauri::command]
pub async fn choose_download_dir(app: AppHandle, state: St<'_>) -> Result<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let Some(folder) = rx.await.map_err(|e| AppError::Invalid(e.to_string()))? else {
        return Ok(None);
    };
    let path = folder.into_path().map_err(|e| AppError::Invalid(e.to_string()))?;
    std::fs::create_dir_all(&path)?;
    let probe = path.join(".typevault-write-test");
    std::fs::write(&probe, b"ok").map_err(|_| AppError::Invalid("Bu klasöre yazılamıyor".into()))?;
    let _ = std::fs::remove_file(&probe);
    let text = path.to_string_lossy().to_string();
    state.db.set_setting("download_dir", &text)?;
    Ok(Some(text))
}

#[tauri::command]
pub fn reset_download_dir(state: St<'_>) -> Result<()> {
    state.db.set_setting("download_dir", "")
}

fn dir_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => dir_size(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    fonts_bytes: u64,
    installed_bytes: u64,
    cache_bytes: u64,
}

#[tauri::command]
pub async fn storage_info(state: St<'_>) -> Result<StorageInfo> {
    let fonts = state.font_dir();
    let cache = state.cache_dir.join("previews");
    let installed = user_font_root()?;
    tokio::task::spawn_blocking(move || StorageInfo {
        fonts_bytes: dir_size(&fonts),
        installed_bytes: dir_size(&installed),
        cache_bytes: dir_size(&cache),
    })
    .await
    .map_err(|e| AppError::Invalid(e.to_string()))
}

#[tauri::command]
pub async fn clear_cache(state: St<'_>) -> Result<()> {
    let dir: PathBuf = state.cache_dir.join("previews");
    match tokio::fs::remove_dir_all(&dir).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRow {
    id: String,
    family: String,
    category: String,
    license: Option<String>,
    local_status: String,
    turkish_status: String,
}

#[tauri::command]
pub async fn export_library(app: AppHandle, state: St<'_>) -> Result<Option<String>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_file_name("typevault-kitaplik.json").save_file(move |file| {
        let _ = tx.send(file);
    });
    let Some(file) = rx.await.map_err(|e| AppError::Invalid(e.to_string()))? else {
        return Ok(None);
    };
    let path = file.into_path().map_err(|e| AppError::Invalid(e.to_string()))?;
    let rows: Vec<ExportRow> = state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT f.id, f.family, f.category, l.name, f.local_status, f.turkish_status
                 FROM fonts f LEFT JOIN licenses l ON l.id=f.license_id
                 WHERE f.local_status<>'available' OR EXISTS (SELECT 1 FROM favorites v WHERE v.font_id=f.id)
                 ORDER BY f.family",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(ExportRow {
                    id: r.get(0)?,
                    family: r.get(1)?,
                    category: r.get(2)?,
                    license: r.get(3)?,
                    local_status: r.get(4)?,
                    turkish_status: r.get(5)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
        .await?;
    tokio::fs::write(&path, serde_json::to_vec_pretty(&rows)?).await?;
    Ok(Some(path.to_string_lossy().to_string()))
}

#[tauri::command]
pub fn reveal_installed_folder(app: AppHandle) -> Result<()> {
    let root = user_font_root()?;
    std::fs::create_dir_all(&root)?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Invalid(e.to_string()))
}

#[tauri::command]
pub fn reveal_download_folder(app: AppHandle, state: St<'_>) -> Result<()> {
    let root = state.font_dir();
    std::fs::create_dir_all(&root)?;
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Invalid(e.to_string()))
}
