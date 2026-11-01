use crate::database::Conn;
use crate::error::{AppError, Result};
use crate::fonts::query::{self, FontPage, FontQuery, FontSummary};
use crate::metadata::{google, sync};
use crate::security::check_font_id;
use crate::state::AppState;
use rusqlite::OptionalExtension;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, State};

type St<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub async fn list_fonts(state: St<'_>, query: FontQuery) -> Result<FontPage> {
    state.db.run(move |conn| query::list(conn, &query)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    weight: i64,
    style: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Axis {
    tag: String,
    name: String,
    min: f64,
    max: f64,
    default_value: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    id: i64,
    filename: String,
    format: String,
    is_variable: bool,
    size_bytes: i64,
    weight: Option<i64>,
    italic: bool,
    postscript_name: Option<String>,
    installed: Option<String>,
    owner: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontDetail {
    summary: FontSummary,
    designers: Vec<String>,
    subsets: Vec<String>,
    languages: Vec<String>,
    description: Option<String>,
    license_url: Option<String>,
    last_modified: Option<String>,
    source: String,
    turkish_source: Option<String>,
    variants: Vec<Variant>,
    axes: Vec<Axis>,
    files: Vec<LocalFile>,
}

fn load_detail(conn: &Conn, id: &str) -> Result<FontDetail> {
    let summary = query::summary(conn, id)?.ok_or_else(|| AppError::NotFound(id.to_string()))?;
    let (designers, subsets, languages, description, license_url, last_modified, source, turkish_source): (
        String, String, String, Option<String>, Option<String>, Option<String>, String, Option<String>,
    ) = conn.query_row(
        "SELECT f.designers, f.subsets, f.languages, f.description, l.url, f.last_modified, f.source, f.turkish_source
         FROM fonts f LEFT JOIN licenses l ON l.id=f.license_id WHERE f.id=?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
    )?;
    let mut stmt = conn.prepare("SELECT weight, style FROM font_variants WHERE font_id=?1 ORDER BY style, weight")?;
    let variants = stmt
        .query_map([id], |r| Ok(Variant { weight: r.get(0)?, style: r.get(1)? }))?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT tag, name, min_value, max_value, default_value FROM font_axes WHERE font_id=?1 ORDER BY tag",
    )?;
    let axes = stmt
        .query_map([id], |r| {
            Ok(Axis { tag: r.get(0)?, name: r.get(1)?, min: r.get(2)?, max: r.get(3)?, default_value: r.get(4)? })
        })?
        .collect::<std::result::Result<_, _>>()?;
    let mut stmt = conn.prepare(
        "SELECT ff.id, ff.filename, ff.format, ff.is_variable, ff.size_bytes, ff.weight, ff.italic, ff.postscript_name,
                i.status, i.owner
         FROM font_files ff LEFT JOIN installations i ON i.file_id=ff.id
         WHERE ff.font_id=?1 AND ff.local_path IS NOT NULL ORDER BY ff.italic, ff.weight, ff.filename",
    )?;
    let files = stmt
        .query_map([id], |r| {
            Ok(LocalFile {
                id: r.get(0)?,
                filename: r.get(1)?,
                format: r.get(2)?,
                is_variable: r.get(3)?,
                size_bytes: r.get(4)?,
                weight: r.get(5)?,
                italic: r.get(6)?,
                postscript_name: r.get(7)?,
                installed: r.get(8)?,
                owner: r.get(9)?,
            })
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(FontDetail {
        summary,
        designers: serde_json::from_str(&designers).unwrap_or_default(),
        subsets: serde_json::from_str(&subsets).unwrap_or_default(),
        languages: serde_json::from_str(&languages).unwrap_or_default(),
        description,
        license_url,
        last_modified,
        source,
        turkish_source,
        variants,
        axes,
        files,
    })
}

#[tauri::command]
pub async fn get_font(state: St<'_>, id: String) -> Result<FontDetail> {
    check_font_id(&id)?;
    state.db.run(move |conn| load_detail(conn, &id)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    total: i64,
    enriched: i64,
    syncing: bool,
    last_sync: Option<String>,
    turkish_full: i64,
    categories: Vec<CategoryCount>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    id: String,
    count: i64,
}

#[tauri::command]
pub async fn catalog_status(state: St<'_>) -> Result<CatalogStatus> {
    let syncing = state.syncing.load(Ordering::SeqCst);
    state
        .db
        .run(move |conn| {
            let (total, enriched, turkish_full) = conn.query_row(
                "SELECT count(*), count(enriched_version), coalesce(sum(turkish_status='full'),0) FROM fonts WHERE in_catalog=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let last_sync = conn
                .query_row("SELECT value FROM settings WHERE key='last_sync'", [], |r| r.get(0))
                .optional()?;
            let mut stmt = conn.prepare("SELECT category, count(*) FROM font_categories GROUP BY category")?;
            let categories = stmt
                .query_map([], |r| Ok(CategoryCount { id: r.get(0)?, count: r.get(1)? }))?
                .collect::<std::result::Result<_, _>>()?;
            Ok(CatalogStatus { total, enriched, syncing, last_sync, turkish_full, categories })
        })
        .await
}

#[tauri::command]
pub fn sync_catalog(app: AppHandle, state: St<'_>) {
    sync::spawn_sync(app, state.inner().clone());
}

#[tauri::command]
pub async fn test_api_key(state: St<'_>, key: String) -> Result<usize> {
    if key.len() > 128 || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::Invalid("API anahtarı biçimi geçersiz".into()));
    }
    Ok(google::fetch_developer_api(&state.http, &key).await?.len())
}
