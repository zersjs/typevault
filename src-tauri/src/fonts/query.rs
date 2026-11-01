//! Katalog sorguları. Filtreler parametrelerle bağlanır, sıralama yalnız izinli değerlerden seçilir.
use crate::database::Conn;
use crate::error::Result;
use rusqlite::{params_from_iter, types::Value, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FontQuery {
    pub search: String,
    pub category: Option<String>,
    pub turkish: Option<String>,
    pub local_status: Option<String>,
    pub collection_id: Option<i64>,
    pub favorites: bool,
    pub variable_only: bool,
    pub updates_only: bool,
    pub local_only: bool,
    pub turkish_first: bool,
    pub sort: Option<String>,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontSummary {
    pub id: String,
    pub family: String,
    pub category: String,
    pub categories: Vec<String>,
    pub variant_count: i64,
    pub is_variable: bool,
    pub turkish_status: String,
    pub turkish_missing: String,
    pub local_status: String,
    pub local_error: Option<String>,
    pub update_available: bool,
    pub favorite: bool,
    pub license: Option<String>,
    pub download_status: Option<String>,
    pub download_id: Option<i64>,
    pub preview_weight: i64,
    pub preview_italic: bool,
    pub preview_file_id: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct FontPage {
    pub items: Vec<FontSummary>,
    pub total: i64,
}

const SELECT: &str = "SELECT f.id, f.family, f.category,
    (SELECT group_concat(c.category) FROM font_categories c WHERE c.font_id=f.id),
    (SELECT count(*) FROM font_variants v WHERE v.font_id=f.id),
    f.is_variable, f.turkish_status, f.turkish_missing, f.local_status, f.local_error, f.update_available,
    EXISTS (SELECT 1 FROM favorites fv WHERE fv.font_id=f.id),
    (SELECT l.name FROM licenses l WHERE l.id=f.license_id),
    d.status, d.id,
    COALESCE((SELECT v.weight FROM font_variants v WHERE v.font_id=f.id ORDER BY v.style='italic', abs(v.weight-400) LIMIT 1), 400),
    COALESCE((SELECT v.style='italic' FROM font_variants v WHERE v.font_id=f.id ORDER BY v.style='italic', abs(v.weight-400) LIMIT 1), 0),
    (SELECT ff.id FROM font_files ff WHERE ff.font_id=f.id AND ff.local_path IS NOT NULL
       ORDER BY ff.is_variable DESC, ff.italic, abs(COALESCE(ff.weight,400)-400) LIMIT 1)
  FROM fonts f
  LEFT JOIN downloads d ON d.font_id=f.id AND d.status IN ('queued','downloading','paused')";

pub fn row_to_summary(r: &Row) -> rusqlite::Result<FontSummary> {
    let categories: Option<String> = r.get(3)?;
    Ok(FontSummary {
        id: r.get(0)?,
        family: r.get(1)?,
        category: r.get(2)?,
        categories: categories.map(|c| c.split(',').map(String::from).collect()).unwrap_or_default(),
        variant_count: r.get(4)?,
        is_variable: r.get(5)?,
        turkish_status: r.get(6)?,
        turkish_missing: r.get(7)?,
        local_status: r.get(8)?,
        local_error: r.get(9)?,
        update_available: r.get(10)?,
        favorite: r.get(11)?,
        license: r.get(12)?,
        download_status: r.get(13)?,
        download_id: r.get(14)?,
        preview_weight: r.get(15)?,
        preview_italic: r.get(16)?,
        preview_file_id: r.get(17)?,
    })
}

/// Kullanıcı girdisini FTS5 önek sorgusuna çevirir. Türkçe ı/İ, ayırıcı işaret değildir ve elle katlanır.
pub fn fts_query(input: &str) -> Option<String> {
    let folded: String = input
        .chars()
        .map(|c| match c {
            'ı' | 'İ' | 'I' => 'i',
            other => other,
        })
        .collect();
    let tokens: Vec<String> = folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\"*"))
        .collect();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

fn sort_clause(sort: Option<&str>) -> &'static str {
    match sort {
        Some("name") => "f.family COLLATE NOCASE",
        Some("newest") => "f.date_added DESC, f.popularity",
        Some("trending") => "f.trending, f.popularity",
        Some("added") => "cf.position",
        _ => "f.popularity, f.family",
    }
}

struct Filter {
    from: String,
    conditions: Vec<String>,
    args: Vec<Value>,
}

fn build_filter(q: &FontQuery) -> Filter {
    let mut f = Filter { from: String::new(), conditions: Vec::new(), args: Vec::new() };
    if let Some(search) = fts_query(&q.search) {
        f.conditions.push("f.rowid IN (SELECT rowid FROM fonts_fts WHERE fonts_fts MATCH ?)".into());
        f.args.push(search.into());
    }
    if let Some(category) = &q.category {
        f.conditions.push("EXISTS (SELECT 1 FROM font_categories c WHERE c.font_id=f.id AND c.category=?)".into());
        f.args.push(category.clone().into());
    }
    if let Some(turkish) = &q.turkish {
        f.conditions.push("f.turkish_status=?".into());
        f.args.push(turkish.clone().into());
    }
    if let Some(status) = &q.local_status {
        f.conditions.push("f.local_status=?".into());
        f.args.push(status.clone().into());
    }
    if let Some(id) = q.collection_id {
        f.from = " JOIN collection_fonts cf ON cf.font_id=f.id AND cf.collection_id=?".into();
        f.args.insert(0, id.into());
    }
    if q.favorites {
        f.conditions.push("EXISTS (SELECT 1 FROM favorites fv WHERE fv.font_id=f.id)".into());
    }
    if q.variable_only {
        f.conditions.push("f.is_variable=1".into());
    }
    if q.updates_only {
        f.conditions.push("f.update_available=1".into());
    }
    if q.local_only {
        f.conditions.push("f.local_status<>'available'".into());
    } else {
        f.conditions.push("(f.in_catalog=1 OR f.local_status<>'available')".into());
    }
    f
}

pub fn list(conn: &Conn, q: &FontQuery) -> Result<FontPage> {
    let filter = build_filter(q);
    let where_sql = if filter.conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filter.conditions.join(" AND "))
    };
    let sort = q.sort.as_deref().filter(|s| *s != "added" || q.collection_id.is_some());
    let sort = if sort.is_none() && q.collection_id.is_some() { "cf.position" } else { sort_clause(sort) };
    let priority = if q.turkish_first { "f.turkish_status='full' DESC, " } else { "" };
    let limit = q.limit.clamp(1, 500);
    let sql = format!("{SELECT}{}{where_sql} ORDER BY {priority}{sort} LIMIT {limit} OFFSET {}", filter.from, q.offset.max(0));
    let mut stmt = conn.prepare_cached(&sql)?;
    let items = stmt
        .query_map(params_from_iter(filter.args.iter()), row_to_summary)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let count_sql = format!("SELECT count(*) FROM fonts f{}{where_sql}", filter.from);
    let total = conn.query_row(&count_sql, params_from_iter(filter.args.iter()), |r| r.get(0))?;
    Ok(FontPage { items, total })
}

/// Toplu işlemler için filtreye uyan tüm kimlikler.
pub fn ids(conn: &Conn, q: &FontQuery) -> Result<Vec<String>> {
    let filter = build_filter(q);
    let where_sql = format!(" WHERE {}", filter.conditions.join(" AND "));
    let sql = format!("SELECT f.id FROM fonts f{}{where_sql} ORDER BY f.popularity LIMIT 5000", filter.from);
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(filter.args.iter()), |r| r.get(0))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn summary(conn: &Conn, id: &str) -> Result<Option<FontSummary>> {
    use rusqlite::OptionalExtension;
    let sql = format!("{SELECT} WHERE f.id=?1");
    Ok(conn.query_row(&sql, [id], row_to_summary).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::test_db;
    use rusqlite::params;

    #[test]
    fn fts_query_is_safe_and_turkish_aware() {
        assert_eq!(fts_query("roboto sl").unwrap(), "\"roboto\"* \"sl\"*");
        assert_eq!(fts_query("İnter").unwrap(), "\"inter\"*");
        assert_eq!(fts_query("sığ").unwrap(), "\"siğ\"*");
        assert_eq!(fts_query("a\" OR b*").unwrap(), "\"a\"* \"OR\"* \"b\"*");
        assert!(fts_query(" ** ").is_none());
    }

    #[test]
    fn filters_and_counts() {
        let (_d, db) = test_db();
        let conn = db.conn().unwrap();
        for (i, name) in ["Roboto", "Lora", "Inter"].iter().enumerate() {
            let id = format!("gf:{}", name.to_lowercase());
            conn.execute(
                "INSERT INTO fonts (id,family,slug,category,popularity,turkish_status) VALUES (?1,?2,?3,'Serif',?4,?5)",
                params![id, name, name.to_lowercase(), i as i64, if i == 0 { "full" } else { "unknown" }],
            )
            .unwrap();
            let rowid: i64 = conn.query_row("SELECT rowid FROM fonts WHERE id=?1", [&id], |r| r.get(0)).unwrap();
            conn.execute("INSERT INTO fonts_fts (rowid,family,designers,keywords) VALUES (?1,?2,'','serif')", params![rowid, name]).unwrap();
        }
        conn.execute("INSERT INTO favorites (font_id) VALUES ('gf:lora')", []).unwrap();
        let page = list(&conn, &FontQuery { limit: 10, ..Default::default() }).unwrap();
        assert_eq!(page.total, 3);
        assert_eq!(page.items[0].family, "Roboto");
        let page = list(&conn, &FontQuery { limit: 10, search: "İnt".into(), ..Default::default() }).unwrap();
        assert_eq!(page.items[0].family, "Inter");
        let page = list(&conn, &FontQuery { limit: 10, turkish: Some("full".into()), ..Default::default() }).unwrap();
        assert_eq!(page.total, 1);
        let page = list(&conn, &FontQuery { limit: 10, favorites: true, ..Default::default() }).unwrap();
        assert_eq!(page.items[0].family, "Lora");
        assert!(page.items[0].favorite);
    }
}
