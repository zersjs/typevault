//! Katalog eşitleme ve aile başına ayrıntı (lisans + glif kapsamı) zenginleştirmesi.
use super::google::{self, AxisInfo, Family, FamilyDetail};
use crate::database::{set_setting, Conn};
use crate::error::{AppError, Result};
use crate::fonts::{taxonomy, turkish};
use crate::security::slugify;
use crate::state::AppState;
use futures_util::{stream, StreamExt};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

const ENRICH_CONCURRENCY: usize = 6;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub phase: &'static str,
    pub done: i64,
    pub total: i64,
    pub message: Option<String>,
}

fn emit(app: &AppHandle, phase: &'static str, done: i64, total: i64, message: Option<String>) {
    let _ = app.emit("catalog://progress", SyncProgress { phase, done, total, message });
}

/// Aynı anda tek eşitleme çalışır; ikinci çağrı sessizce döner.
pub fn spawn_sync(app: AppHandle, state: Arc<AppState>) {
    if state.syncing.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let result = run_sync(&app, &state).await;
        state.syncing.store(false, Ordering::SeqCst);
        if let Err(e) = result {
            emit(&app, "error", 0, 0, Some(e.to_string()));
        }
    });
}

async fn run_sync(app: &AppHandle, state: &Arc<AppState>) -> Result<()> {
    emit(app, "catalog", 0, 0, None);
    let key = state.db.setting("google_api_key")?.filter(|k| !k.trim().is_empty());
    let catalog = google::fetch_catalog(&state.http).await;
    let (mut families, axis_info) = match catalog {
        Ok(c) => (c.family_metadata_list, c.axis_registry),
        Err(e) if key.is_none() => return Err(e),
        Err(_) => (Vec::new(), Vec::new()),
    };
    if let Some(key) = key {
        merge_api(&mut families, google::fetch_developer_api(&state.http, key.trim()).await?);
    }
    if families.is_empty() {
        return Err(AppError::Invalid("Katalog boş döndü".into()));
    }
    emit(app, "tags", 0, 0, None);
    // Etiketler ek sınıflandırma sağlar; alınamazsa önceki etiketler korunur.
    let tags = google::fetch_tags(&state.http).await.ok();
    let total = families.len() as i64;
    emit(app, "store", 0, total, None);
    state.db.run(move |conn| store_catalog(conn, &families, &axis_info, tags.as_ref())).await?;
    state.db.run(rebuild_collections).await?;
    let _ = app.emit("library://changed", ());
    enrich(app, state).await?;
    state.db.set_setting("last_sync", &chrono::Utc::now().to_rfc3339())?;
    emit(app, "done", total, total, None);
    Ok(())
}

fn merge_api(families: &mut Vec<Family>, api: Vec<Family>) {
    let mut index: HashMap<String, usize> =
        families.iter().enumerate().map(|(i, f)| (f.family.clone(), i)).collect();
    for family in api {
        match index.get(&family.family) {
            Some(i) => {
                let existing = &mut families[*i];
                existing.api_files = family.api_files;
                existing.last_modified = family.last_modified.or(existing.last_modified.take());
            }
            None => {
                index.insert(family.family.clone(), families.len());
                families.push(family);
            }
        }
    }
}

fn parse_variant(key: &str) -> Option<(i64, &'static str)> {
    let (digits, style) = match key.strip_suffix('i') {
        Some(d) => (d, "italic"),
        None => (key, "normal"),
    };
    Some((digits.parse().ok()?, style))
}

fn store_catalog(
    conn: &mut Conn,
    families: &[Family],
    axis_info: &[AxisInfo],
    tags: Option<&HashMap<String, HashMap<String, i64>>>,
) -> Result<()> {
    let axis_names: HashMap<&str, &str> =
        axis_info.iter().map(|a| (a.tag.as_str(), a.display_name.as_str())).collect();
    let empty = HashMap::new();
    let tx = conn.transaction()?;
    tx.execute("UPDATE fonts SET in_catalog = 0", [])?;
    {
        let mut upsert = tx.prepare(
            "INSERT INTO fonts (id, family, slug, category, designers, subsets, popularity, trending, date_added,
               last_modified, size_bytes, is_variable, is_noto, is_color, api_files, in_catalog)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1)
             ON CONFLICT(id) DO UPDATE SET family=excluded.family, category=excluded.category,
               designers=excluded.designers, subsets=excluded.subsets, popularity=excluded.popularity,
               trending=excluded.trending, date_added=excluded.date_added, last_modified=excluded.last_modified,
               size_bytes=excluded.size_bytes, is_variable=excluded.is_variable, is_noto=excluded.is_noto,
               is_color=excluded.is_color, api_files=excluded.api_files, in_catalog=1,
               update_available = CASE WHEN fonts.downloaded_version IS NOT NULL
                 AND fonts.downloaded_version <> excluded.last_modified THEN 1 ELSE 0 END",
        )?;
        let mut del_children = tx.prepare(
            "DELETE FROM font_variants WHERE font_id=?1",
        )?;
        let mut del_axes = tx.prepare("DELETE FROM font_axes WHERE font_id=?1")?;
        let mut del_cats = tx.prepare("DELETE FROM font_categories WHERE font_id=?1")?;
        let mut del_tags = tx.prepare("DELETE FROM font_tags WHERE font_id=?1")?;
        let mut ins_variant = tx.prepare(
            "INSERT OR REPLACE INTO font_variants (font_id, weight, style, width_class, thickness) VALUES (?1,?2,?3,?4,?5)",
        )?;
        let mut ins_axis = tx.prepare(
            "INSERT OR REPLACE INTO font_axes (font_id, tag, name, min_value, max_value, default_value) VALUES (?1,?2,?3,?4,?5,?6)",
        )?;
        let mut ins_cat = tx.prepare("INSERT OR IGNORE INTO font_categories (font_id, category) VALUES (?1,?2)")?;
        let mut ins_tag = tx.prepare("INSERT OR REPLACE INTO font_tags (font_id, tag, weight) VALUES (?1,?2,?3)")?;
        let mut read_tags = tx.prepare("SELECT tag, weight FROM font_tags WHERE font_id=?1")?;
        for f in families {
            let slug = slugify(&f.family);
            if slug.is_empty() {
                continue;
            }
            let id = format!("gf:{slug}");
            upsert.execute(params![
                id,
                f.family,
                slug,
                f.category,
                serde_json::to_string(&f.designers)?,
                serde_json::to_string(&f.subsets)?,
                f.popularity.unwrap_or(100_000),
                f.trending.unwrap_or(100_000),
                f.date_added,
                f.last_modified,
                f.size,
                !f.axes.is_empty(),
                f.is_noto,
                !f.color_capabilities.is_empty(),
                f.api_files.as_ref().map(serde_json::to_string).transpose()?,
            ])?;
            del_children.execute([&id])?;
            del_axes.execute([&id])?;
            del_cats.execute([&id])?;
            let family_tags: HashMap<String, i64> = match tags {
                Some(all) => {
                    del_tags.execute([&id])?;
                    let current = all.get(&f.family).unwrap_or(&empty);
                    for (tag, weight) in current {
                        ins_tag.execute(params![id, tag, weight])?;
                    }
                    current.clone()
                }
                None => read_tags
                    .query_map([&id], |r| Ok((r.get(0)?, r.get(1)?)))?
                    .collect::<std::result::Result<_, _>>()?,
            };
            let mut width_sum = 0.0;
            let mut width_n = 0.0;
            for (key, variant) in &f.fonts {
                let Some((weight, style)) = parse_variant(key) else { continue };
                ins_variant.execute(params![id, weight, style, variant.width, variant.thickness])?;
                if let Some(w) = variant.width {
                    width_sum += w as f64;
                    width_n += 1.0;
                }
            }
            for axis in &f.axes {
                let name = axis_names.get(axis.tag.as_str()).copied().unwrap_or(&axis.tag);
                ins_axis.execute(params![id, axis.tag, name, axis.min, axis.max, axis.default_value])?;
            }
            let categories = taxonomy::classify(&taxonomy::Input {
                family: &f.family,
                category: &f.category,
                tags: &family_tags,
                has_axes: !f.axes.is_empty(),
                narrow_width_axis: f.axes.iter().any(|a| a.tag == "wdth" && a.min < 90.0),
                avg_width: (width_n > 0.0).then(|| width_sum / width_n),
            });
            for category in &categories {
                ins_cat.execute(params![id, category])?;
            }
        }
    }
    tx.execute_batch(
        "DELETE FROM fonts_fts;
         DELETE FROM font_categories WHERE font_id IN (SELECT id FROM fonts WHERE in_catalog = 0 AND local_status = 'available');",
    )?;
    {
        let mut rows = tx.prepare(
            "SELECT f.rowid, f.family, f.designers, group_concat(c.category, ' ')
             FROM fonts f LEFT JOIN font_categories c ON c.font_id = f.id GROUP BY f.id",
        )?;
        let mut ins = tx.prepare("INSERT INTO fonts_fts (rowid, family, designers, keywords) VALUES (?1,?2,?3,?4)")?;
        let items = rows.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        for item in items {
            let (rowid, family, designers, cats) = item?;
            let cats = cats.unwrap_or_default();
            let cat_list: Vec<&str> = cats.split(' ').collect();
            let designers: Vec<String> = serde_json::from_str(&designers).unwrap_or_default();
            ins.execute(params![rowid, family, designers.join(" "), taxonomy::keywords(&cat_list)])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn rebuild_collections(conn: &mut Conn) -> Result<()> {
    let tx = conn.transaction()?;
    for (position, c) in taxonomy::SYSTEM_COLLECTIONS.iter().enumerate() {
        tx.execute(
            "INSERT INTO collections (slug, name, description, is_system, position) VALUES (?1,?2,?3,1,?4)
             ON CONFLICT(slug) DO UPDATE SET name=excluded.name, description=excluded.description, position=excluded.position",
            params![c.slug, c.name, c.description, position as i64],
        )?;
        let id: i64 = tx.query_row("SELECT id FROM collections WHERE slug=?1", [c.slug], |r| r.get(0))?;
        tx.execute("DELETE FROM collection_fonts WHERE collection_id=?1", [id])?;
        tx.execute(
            &format!(
                "INSERT INTO collection_fonts (collection_id, font_id, position)
                 SELECT ?1, id, row_number() OVER (ORDER BY score DESC) FROM ({}) ORDER BY score DESC LIMIT ?2",
                c.sql
            ),
            params![id, taxonomy::collection_limit(c.slug)],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn license_id(conn: &Conn, code: &str) -> Result<Option<i64>> {
    Ok(conn.query_row("SELECT id FROM licenses WHERE code=?1", [code], |r| r.get(0)).optional()?)
}

pub fn apply_detail(conn: &Conn, id: &str, version: Option<&str>, detail: &FamilyDetail) -> Result<()> {
    let license = detail.license.as_deref().map(|c| license_id(conn, c)).transpose()?.flatten();
    conn.execute(
        "UPDATE fonts SET license_id = COALESCE(?2, license_id), description=?3, languages=?4, enriched_version=?5 WHERE id=?1",
        params![id, license, detail.description, serde_json::to_string(&detail.languages)?, version],
    )?;
    if !detail.coverage.is_empty() {
        let support = turkish::evaluate(|c| {
            detail.coverage.values().any(|ranges| turkish::ranges_contain(ranges, c as u32))
        });
        // İkili dosyadan okunan cmap sonucu, kapsam verisinden daha güvenilirdir.
        conn.execute(
            "UPDATE fonts SET turkish_status=?2, turkish_missing=?3, turkish_source='coverage'
             WHERE id=?1 AND COALESCE(turkish_source,'') <> 'cmap'",
            params![id, support.status, support.missing],
        )?;
    }
    Ok(())
}

/// Lisans ve glif kapsamını aileler için tek tek getirir; yarıda kalırsa kaldığı yerden sürer.
async fn enrich(app: &AppHandle, state: &Arc<AppState>) -> Result<()> {
    let pending: Vec<(String, String, Option<String>)> = state
        .db
        .run(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, family, last_modified FROM fonts
                 WHERE in_catalog=1 AND (enriched_version IS NULL OR enriched_version <> COALESCE(last_modified,''))
                 ORDER BY popularity",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            Ok(rows.collect::<std::result::Result<_, _>>()?)
        })
        .await?;
    let total = pending.len() as i64;
    if total == 0 {
        return Ok(());
    }
    let mut done = 0;
    let mut failures = 0;
    let http = state.http.clone();
    let mut results = stream::iter(pending)
        .map(|(id, family, version)| {
            let http = http.clone();
            async move {
                let detail = google::fetch_detail(&http, &family).await;
                (id, version, detail)
            }
        })
        .buffer_unordered(ENRICH_CONCURRENCY);
    while let Some((id, version, detail)) = results.next().await {
        done += 1;
        match detail {
            Ok(detail) => {
                state
                    .db
                    .run(move |conn| apply_detail(conn, &id, version.as_deref().or(Some("")), &detail))
                    .await?;
            }
            Err(_) => failures += 1,
        }
        if done % 25 == 0 || done == total {
            emit(app, "enrich", done, total, None);
        }
        if done % 250 == 0 {
            state.db.run(rebuild_collections).await?;
            let _ = app.emit("library://changed", ());
        }
    }
    state.db.run(rebuild_collections).await?;
    let _ = app.emit("library://changed", ());
    if failures > 0 {
        let conn = state.db.conn()?;
        set_setting(&conn, "enrich_failures", &failures.to_string())?;
    }
    Ok(())
}
