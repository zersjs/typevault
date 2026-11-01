//! İndirme, kurulum ve toplu işlem komutları.
use super::check_ids;
use crate::downloader::{create_batch, mark_batch, Progress};
use crate::error::{AppError, Result};
use crate::fonts::query::{self, FontQuery};
use crate::security::{check_font_id, is_inside};
use crate::state::AppState;
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

type St<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn download_font(state: St<'_>, id: String, install_after: bool) -> Result<()> {
    check_font_id(&id)?;
    state.downloads.enqueue(&id, install_after, None)
}

#[tauri::command]
pub fn install_font(state: St<'_>, id: String) -> Result<()> {
    check_font_id(&id)?;
    let conn = state.db.conn()?;
    let (present, missing): (i64, i64) = conn.query_row(
        "SELECT coalesce(sum(local_path IS NOT NULL),0), coalesce(sum(local_path IS NULL),0) FROM font_files WHERE font_id=?1",
        [&id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if present > 0 && missing == 0 {
        state.installer.enqueue(id, None);
        Ok(())
    } else {
        state.downloads.enqueue(&id, true, None)
    }
}

#[tauri::command]
pub async fn uninstall_font(state: St<'_>, id: String) -> Result<usize> {
    check_font_id(&id)?;
    let installer = state.installer.clone();
    tokio::task::spawn_blocking(move || installer.uninstall(&id))
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
}

/// Kurulu olmayan fontun indirilmiş dosyalarını siler.
#[tauri::command]
pub async fn delete_downloaded(state: St<'_>, id: String) -> Result<()> {
    check_font_id(&id)?;
    let root = state.font_dir();
    let fallback = state.default_font_dir.clone();
    state
        .db
        .run(move |conn| {
            let installed: i64 = conn.query_row(
                "SELECT count(*) FROM installations WHERE font_id=?1 AND status='installed'",
                [&id],
                |r| r.get(0),
            )?;
            if installed > 0 {
                return Err(AppError::Invalid("Önce fontu kaldırın".into()));
            }
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let paths: Vec<String> = {
                let mut stmt = tx.prepare("SELECT local_path FROM font_files WHERE font_id=?1 AND local_path IS NOT NULL")?;
                let rows = stmt.query_map([&id], |r| r.get(0))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            tx.execute("DELETE FROM font_files WHERE font_id=?1", [&id])?;
            tx.execute(
                "UPDATE fonts SET local_status='available', local_error=NULL, downloaded_version=NULL, update_available=0 WHERE id=?1",
                [&id],
            )?;
            // Geri alınabilecek işlem bitmeden dosya silinmez; kayıt ve disk ayrışmasın.
            tx.commit()?;
            for path in paths {
                let path = PathBuf::from(path);
                if is_inside(&root, &path) || is_inside(&fallback, &path) {
                    let _ = std::fs::remove_file(&path);
                    if let Some(parent) = path.parent() {
                        let only_license = std::fs::read_dir(parent)
                            .map(|d| d.flatten().all(|e| e.path().extension().is_some_and(|x| x == "txt")))
                            .unwrap_or(false);
                        if only_license {
                            let _ = std::fs::remove_dir_all(parent);
                        }
                    }
                }
            }
            Ok(())
        })
        .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkRequest {
    label: String,
    #[serde(default)]
    ids: Vec<String>,
    query: Option<FontQuery>,
    install: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkStart {
    batch_id: i64,
    total: usize,
}

/// Seçili fontların eksiklerini indirir, gerekiyorsa kurar. Tek bir ilerleme kaydı oluşturur.
#[tauri::command]
pub async fn start_bulk(app: AppHandle, state: St<'_>, request: BulkRequest) -> Result<BulkStart> {
    check_ids(&request.ids)?;
    let label: String = request.label.trim().chars().take(100).collect();
    let state = state.inner().clone();
    let db = state.db.clone();
    let ids = match request.query {
        Some(query) => db.run(move |conn| query::ids(conn, &query)).await?,
        None => request.ids,
    };
    if ids.is_empty() {
        return Err(AppError::Invalid("Seçili font yok".into()));
    }
    let total = ids.len();
    let install = request.install;
    let worker = state.clone();
    let batch_id = tokio::task::spawn_blocking(move || -> Result<i64> {
        let batch = create_batch(&worker.db, &label, &ids, install)?;
        let conn = worker.db.conn()?;
        for id in &ids {
            let (status, files): (String, i64) = conn.query_row(
                "SELECT f.local_status, (SELECT count(*) FROM font_files ff WHERE ff.font_id=f.id AND ff.local_path IS NOT NULL)
                 FROM fonts f WHERE f.id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if status == "installed" || (!install && files > 0 && status != "install_failed") {
                mark_batch(&worker.db, batch, id, "skipped", Some("Zaten hazır"))?;
            } else if files > 0 {
                worker.installer.enqueue(id.clone(), Some(batch));
            } else {
                worker.downloads.enqueue(id, install, Some(batch))?;
            }
        }
        Ok(batch)
    })
    .await
    .map_err(|e| AppError::Invalid(e.to_string()))??;
    let _ = app.emit("library://changed", ());
    Ok(BulkStart { batch_id, total })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    font_id: String,
    family: String,
    state: String,
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    id: i64,
    label: String,
    total: i64,
    install_after: bool,
    done: i64,
    failed: i64,
    skipped: i64,
    failures: Vec<BatchItem>,
}

#[tauri::command]
pub async fn list_batches(state: St<'_>) -> Result<Vec<Batch>> {
    state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT b.id, b.label, b.total, b.install_after,
                   coalesce(sum(s.state='done'),0), coalesce(sum(s.state='failed'),0), coalesce(sum(s.state='skipped'),0)
                 FROM batches b LEFT JOIN batch_fonts s ON s.batch_id=b.id
                 WHERE b.dismissed=0 GROUP BY b.id ORDER BY b.id DESC LIMIT 10",
            )?;
            let mut batches: Vec<Batch> = stmt
                .query_map([], |r| {
                    Ok(Batch {
                        id: r.get(0)?,
                        label: r.get(1)?,
                        total: r.get(2)?,
                        install_after: r.get(3)?,
                        done: r.get(4)?,
                        failed: r.get(5)?,
                        skipped: r.get(6)?,
                        failures: Vec::new(),
                    })
                })?
                .collect::<std::result::Result<_, _>>()?;
            for b in &mut batches {
                let mut stmt = conn.prepare(
                    "SELECT s.font_id, f.family, s.state, s.message FROM batch_fonts s JOIN fonts f ON f.id=s.font_id
                     WHERE s.batch_id=?1 AND s.state='failed'",
                )?;
                b.failures = stmt
                    .query_map([b.id], |r| {
                        Ok(BatchItem { font_id: r.get(0)?, family: r.get(1)?, state: r.get(2)?, message: r.get(3)? })
                    })?
                    .collect::<std::result::Result<_, _>>()?;
            }
            Ok(batches)
        })
        .await
}

#[tauri::command]
pub async fn dismiss_batch(state: St<'_>, id: i64) -> Result<()> {
    state
        .db
        .run(move |conn| {
            conn.execute("UPDATE batches SET dismissed=1 WHERE id=?1", [id])?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn retry_batch_failures(state: St<'_>, id: i64) -> Result<usize> {
    let worker = state.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<usize> {
        let conn = worker.db.conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.font_id, b.install_after,
               EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=s.font_id)
               AND NOT EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=s.font_id AND ff.local_path IS NULL)
             FROM batch_fonts s JOIN batches b ON b.id=s.batch_id WHERE s.batch_id=?1 AND s.state='failed'",
        )?;
        let items: Vec<(String, bool, bool)> = stmt
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (font, install, has_files) in &items {
            conn.execute(
                "UPDATE batch_fonts SET state='pending', message=NULL WHERE batch_id=?1 AND font_id=?2",
                params![id, font],
            )?;
            if *has_files && *install {
                worker.installer.enqueue(font.clone(), Some(id));
            } else {
                worker.downloads.enqueue(font, *install, Some(id))?;
            }
        }
        Ok(items.len())
    })
    .await
    .map_err(|e| AppError::Invalid(e.to_string()))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRow {
    id: i64,
    font_id: String,
    family: String,
    status: String,
    install_after: bool,
    received_bytes: i64,
    files_done: i64,
    files_total: i64,
    error: Option<String>,
    local_status: String,
    updated_at: String,
}

#[tauri::command]
pub async fn list_downloads(state: St<'_>) -> Result<Vec<DownloadRow>> {
    state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT d.id, d.font_id, f.family, d.status, d.install_after, d.received_bytes, d.files_done,
                   d.files_total, d.error, f.local_status, d.updated_at
                 FROM downloads d JOIN fonts f ON f.id=d.font_id
                 WHERE d.status <> 'cancelled'
                 ORDER BY CASE d.status WHEN 'downloading' THEN 0 WHEN 'queued' THEN 1 WHEN 'paused' THEN 2 WHEN 'failed' THEN 3 ELSE 4 END,
                   d.updated_at DESC LIMIT 1000",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(DownloadRow {
                    id: r.get(0)?,
                    font_id: r.get(1)?,
                    family: r.get(2)?,
                    status: r.get(3)?,
                    install_after: r.get(4)?,
                    received_bytes: r.get(5)?,
                    files_done: r.get(6)?,
                    files_total: r.get(7)?,
                    error: r.get(8)?,
                    local_status: r.get(9)?,
                    updated_at: r.get(10)?,
                })
            })?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
        .await
}

#[tauri::command]
pub fn download_snapshot(state: St<'_>) -> Vec<Progress> {
    state.downloads.snapshot()
}

#[tauri::command]
pub fn pause_download(state: St<'_>, id: i64) -> Result<()> {
    state.downloads.pause(id)
}

#[tauri::command]
pub fn resume_download(state: St<'_>, id: i64) -> Result<()> {
    state.downloads.resume(id)
}

#[tauri::command]
pub fn cancel_download(state: St<'_>, id: i64) -> Result<()> {
    state.downloads.cancel(id)
}

#[tauri::command]
pub fn pause_all(state: St<'_>) -> Result<()> {
    let ids: Vec<i64> = {
        let conn = state.db.conn()?;
        let mut stmt = conn.prepare("SELECT id FROM downloads WHERE status IN ('queued','downloading')")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    ids.into_iter().try_for_each(|id| state.downloads.pause(id))
}

#[tauri::command]
pub fn resume_all(state: St<'_>) -> Result<()> {
    state.db.conn()?.execute("UPDATE downloads SET status='queued', updated_at=datetime('now') WHERE status='paused'", [])?;
    state.downloads.notify();
    Ok(())
}

#[tauri::command]
pub async fn clear_finished_downloads(state: St<'_>) -> Result<()> {
    state
        .db
        .run(|conn| {
            conn.execute("DELETE FROM downloads WHERE status IN ('completed','cancelled','failed')", [])?;
            Ok(())
        })
        .await
}

/// İndirilmiş ancak kurulmamış tüm fontları tek kayıtta kurar.
#[tauri::command]
pub async fn install_all_downloaded(state: St<'_>) -> Result<usize> {
    let ids: Vec<String> = state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare("SELECT id FROM fonts WHERE local_status IN ('downloaded','install_failed')")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
        .await?;
    if ids.is_empty() {
        return Ok(0);
    }
    let batch = create_batch(&state.db, "İndirilen fontlar", &ids, true)?;
    for id in &ids {
        state.installer.enqueue(id.clone(), Some(batch));
    }
    Ok(ids.len())
}
