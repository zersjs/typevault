use super::check_ids;
use crate::error::{AppError, Result};
use crate::fonts::query::{self, FontQuery};
use crate::security::check_font_id;
use crate::state::AppState;
use rusqlite::{params, TransactionBehavior};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

type St<'a> = State<'a, Arc<AppState>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    id: i64,
    name: String,
    description: String,
    is_system: bool,
    font_count: i64,
    installed_count: i64,
    sample_fonts: Vec<SampleFont>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleFont {
    id: String,
    family: String,
    preview_weight: i64,
    preview_italic: bool,
    preview_file_id: Option<i64>,
}

#[tauri::command]
pub async fn list_collections(state: St<'_>) -> Result<Vec<Collection>> {
    state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.name, c.description, c.is_system, count(cf.font_id),
                   coalesce(sum(f.local_status='installed'),0)
                 FROM collections c
                 LEFT JOIN collection_fonts cf ON cf.collection_id=c.id
                 LEFT JOIN fonts f ON f.id=cf.font_id
                 GROUP BY c.id ORDER BY c.is_system DESC, c.position, c.name",
            )?;
            let mut rows: Vec<Collection> = stmt
                .query_map([], |r| {
                    Ok(Collection {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        is_system: r.get(3)?,
                        font_count: r.get(4)?,
                        installed_count: r.get(5)?,
                        sample_fonts: Vec::new(),
                    })
                })?
                .collect::<std::result::Result<_, _>>()?;
            for c in &mut rows {
                let page = query::list(conn, &FontQuery { collection_id: Some(c.id), limit: 3, ..Default::default() })?;
                c.sample_fonts = page
                    .items
                    .into_iter()
                    .map(|f| SampleFont {
                        id: f.id,
                        family: f.family,
                        preview_weight: f.preview_weight,
                        preview_italic: f.preview_italic,
                        preview_file_id: f.preview_file_id,
                    })
                    .collect();
            }
            Ok(rows)
        })
        .await
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(AppError::Invalid("Koleksiyon adı 1–80 karakter olmalı".into()));
    }
    Ok(name.to_string())
}

#[tauri::command]
pub async fn create_collection(state: St<'_>, name: String, font_ids: Vec<String>) -> Result<i64> {
    check_ids(&font_ids)?;
    let name = clean_name(&name)?;
    state
        .db
        .run(move |conn| {
            let tx = conn.transaction()?;
            tx.execute("INSERT INTO collections (name) VALUES (?1)", [&name])?;
            let id = tx.last_insert_rowid();
            for (i, font) in font_ids.iter().enumerate() {
                tx.execute(
                    "INSERT OR IGNORE INTO collection_fonts (collection_id, font_id, position) VALUES (?1,?2,?3)",
                    params![id, font, i as i64],
                )?;
            }
            tx.commit()?;
            Ok(id)
        })
        .await
}

#[tauri::command]
pub async fn rename_collection(state: St<'_>, id: i64, name: String) -> Result<()> {
    let name = clean_name(&name)?;
    state
        .db
        .run(move |conn| {
            conn.execute("UPDATE collections SET name=?2 WHERE id=?1 AND is_system=0", params![id, name])?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn delete_collection(state: St<'_>, id: i64) -> Result<()> {
    state
        .db
        .run(move |conn| {
            conn.execute("DELETE FROM collections WHERE id=?1 AND is_system=0", [id])?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn add_to_collection(state: St<'_>, id: i64, font_ids: Vec<String>) -> Result<()> {
    check_ids(&font_ids)?;
    state
        .db
        .run(move |conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let is_user: bool =
                tx.query_row("SELECT is_system=0 FROM collections WHERE id=?1", [id], |r| r.get(0))?;
            if !is_user {
                return Err(AppError::Invalid("Hazır koleksiyonlar değiştirilemez".into()));
            }
            for font in &font_ids {
                tx.execute(
                    "INSERT OR IGNORE INTO collection_fonts (collection_id, font_id, position)
                     VALUES (?1,?2,(SELECT coalesce(max(position),0)+1 FROM collection_fonts WHERE collection_id=?1))",
                    params![id, font],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn remove_from_collection(state: St<'_>, id: i64, font_ids: Vec<String>) -> Result<()> {
    check_ids(&font_ids)?;
    state
        .db
        .run(move |conn| {
            let is_user: bool =
                conn.query_row("SELECT is_system=0 FROM collections WHERE id=?1", [id], |r| r.get(0))?;
            if !is_user {
                return Err(AppError::Invalid("Hazır koleksiyonlar değiştirilemez".into()));
            }
            for font in &font_ids {
                conn.execute("DELETE FROM collection_fonts WHERE collection_id=?1 AND font_id=?2", params![id, font])?;
            }
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn toggle_favorite(state: St<'_>, id: String) -> Result<bool> {
    check_font_id(&id)?;
    state
        .db
        .run(move |conn| {
            let removed = conn.execute("DELETE FROM favorites WHERE font_id=?1", [&id])?;
            if removed == 0 {
                conn.execute("INSERT INTO favorites (font_id) VALUES (?1)", [&id])?;
            }
            Ok(removed == 0)
        })
        .await
}
