//! `tvfont://` protokolü: yerel dosyaları ve diskte önbelleğe alınan önizleme fontlarını sunar.
use crate::database::Db;
use crate::error::{AppError, Result};
use crate::security::{check_font_id, check_trusted_url};
use crate::state::AppState;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tauri::http::{header, Request, Response, StatusCode};
use url::Url;

/// woff2 ve unicode-range ile küçük alt kümeler almak için güncel bir tarayıcı kimliği.
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

fn respond(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
        .body(body)
        .expect("yanıt")
}

fn hash(value: &str) -> String {
    hex::encode(&Sha256::digest(value.as_bytes())[..16])
}

fn safe_alias(value: &str) -> bool {
    !value.is_empty() && value.len() < 90 && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub async fn handle(state: Arc<AppState>, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match route(&state, &request).await {
        Ok(response) => response,
        Err(AppError::NotFound(_)) => respond(StatusCode::NOT_FOUND, "text/plain", Vec::new()),
        Err(e) => respond(StatusCode::BAD_GATEWAY, "text/plain", e.to_string().into_bytes()),
    }
}

async fn route(state: &Arc<AppState>, request: &Request<Vec<u8>>) -> Result<Response<Vec<u8>>> {
    let url = Url::parse(&request.uri().to_string()).map_err(|_| AppError::NotFound("adres".into()))?;
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let segments: Vec<&str> = url.path_segments().map(|s| s.collect()).unwrap_or_default();
    match segments.as_slice() {
        ["local", id] => local_file(state, id.parse().map_err(|_| AppError::NotFound("kimlik".into()))?).await,
        ["css"] => css(state, &query).await,
        ["asset", key] => asset(state, key).await,
        _ => Err(AppError::NotFound("yol".into())),
    }
}

async fn local_file(state: &Arc<AppState>, id: i64) -> Result<Response<Vec<u8>>> {
    let row: Option<(String, String)> = state
        .db
        .run(move |conn| {
            Ok(conn
                .query_row("SELECT local_path, format FROM font_files WHERE id=?1 AND local_path IS NOT NULL", [id], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?)
        })
        .await?;
    let (path, format) = row.ok_or_else(|| AppError::NotFound("dosya".into()))?;
    let data = tokio::fs::read(&path).await.map_err(|_| AppError::NotFound("dosya".into()))?;
    Ok(respond(StatusCode::OK, if format == "otf" { "font/otf" } else { "font/ttf" }, data))
}

fn font_row(db: &Db, id: &str) -> Result<(String, Option<String>)> {
    db.conn()?
        .query_row("SELECT family, last_modified FROM fonts WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?
        .ok_or_else(|| AppError::NotFound(id.to_string()))
}

/// css2 "family" parametresi; `spec` eksen listesi veya ağırlık dizisidir.
fn css_url(family: &str, spec: Option<&str>) -> String {
    let name = family.replace(' ', "+");
    let spec = spec.filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || "@:,;.-".contains(c)));
    match spec {
        Some(spec) => format!("https://fonts.googleapis.com/css2?family={name}:{spec}&display=swap"),
        None => format!("https://fonts.googleapis.com/css2?family={name}&display=swap"),
    }
}

async fn css(state: &Arc<AppState>, query: &HashMap<String, String>) -> Result<Response<Vec<u8>>> {
    let id = check_font_id(query.get("f").map(String::as_str).unwrap_or_default())?.to_string();
    let alias = query.get("as").cloned().unwrap_or_default();
    if !safe_alias(&alias) {
        return Err(AppError::Invalid("takma ad geçersiz".into()));
    }
    let spec = query.get("spec").map(String::as_str);
    let db = state.db.clone();
    let lookup = id.clone();
    let (family, version) = tokio::task::spawn_blocking(move || font_row(&db, &lookup))
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))??;
    let remote = css_url(&family, spec);
    let dir = state.cache_dir.join("previews");
    tokio::fs::create_dir_all(&dir).await?;
    let key = hash(&format!("{remote}|{}", version.unwrap_or_default()));
    let cached = dir.join(format!("{key}.css"));
    let body = match tokio::fs::read_to_string(&cached).await {
        Ok(body) => body,
        Err(_) => {
            let res = state.http.get(&remote).header(reqwest::header::USER_AGENT, BROWSER_UA).send().await?;
            if !res.status().is_success() {
                return Err(AppError::Status(res.status().as_u16()));
            }
            let body = res.text().await?;
            let rewritten = rewrite_css(&body, &dir).await?;
            tokio::fs::write(&cached, &rewritten).await?;
            rewritten
        }
    };
    let aliased = alias_family(&body, &family, &alias);
    Ok(respond(StatusCode::OK, "text/css; charset=utf-8", aliased.into_bytes()))
}

fn alias_family(css: &str, family: &str, alias: &str) -> String {
    css.replace(&format!("font-family: '{family}'"), &format!("font-family: '{alias}'"))
}

/// Uzak font adreslerini önbellek anahtarlarıyla değiştirir.
async fn rewrite_css(css: &str, dir: &Path) -> Result<String> {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("url(") {
        let (head, tail) = rest.split_at(start + 4);
        out.push_str(head);
        let end = tail.find(')').ok_or_else(|| AppError::Invalid("CSS çözümlenemedi".into()))?;
        let remote = tail[..end].trim_matches(['\'', '"']);
        check_trusted_url(remote)?;
        let key = hash(remote);
        tokio::fs::write(dir.join(format!("{key}.url")), remote).await?;
        out.push_str(&format!("tvfont://localhost/asset/{key}"));
        rest = &tail[end..];
    }
    out.push_str(rest);
    Ok(out)
}

async fn asset(state: &Arc<AppState>, key: &str) -> Result<Response<Vec<u8>>> {
    if !key.chars().all(|c| c.is_ascii_hexdigit()) || key.len() != 32 {
        return Err(AppError::NotFound("anahtar".into()));
    }
    let dir = state.cache_dir.join("previews");
    let path = dir.join(format!("{key}.font"));
    if let Ok(data) = tokio::fs::read(&path).await {
        return Ok(respond(StatusCode::OK, content_type(&data), data));
    }
    let remote = tokio::fs::read_to_string(dir.join(format!("{key}.url")))
        .await
        .map_err(|_| AppError::NotFound("önbellek".into()))?;
    let url = check_trusted_url(remote.trim())?;
    let res = state.http.get(url).send().await?;
    if !res.status().is_success() {
        return Err(AppError::Status(res.status().as_u16()));
    }
    let data = res.bytes().await?.to_vec();
    let tmp = dir.join(format!("{key}.tmp"));
    tokio::fs::write(&tmp, &data).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(respond(StatusCode::OK, content_type(&data), data))
}

fn content_type(data: &[u8]) -> &'static str {
    match data.get(..4) {
        Some(b"wOF2") => "font/woff2",
        Some(b"wOFF") => "font/woff",
        _ => "font/ttf",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_css_urls() {
        assert_eq!(
            css_url("Roboto Slab", Some("wght@100..900")),
            "https://fonts.googleapis.com/css2?family=Roboto+Slab:wght@100..900&display=swap"
        );
        assert_eq!(css_url("Inter", Some("x&y=1")), "https://fonts.googleapis.com/css2?family=Inter&display=swap");
    }

    #[tokio::test]
    async fn rewrites_only_trusted_urls() {
        let dir = tempfile::tempdir().unwrap();
        let ok = "src: url(https://fonts.gstatic.com/s/a.woff2) format('woff2');";
        let out = rewrite_css(ok, dir.path()).await.unwrap();
        assert!(out.starts_with("src: url(tvfont://localhost/asset/"));
        assert!(out.ends_with(") format('woff2');"));
        assert!(rewrite_css("src: url(https://evil.example/a.woff2)", dir.path()).await.is_err());
    }

    #[test]
    fn aliases_family() {
        assert_eq!(alias_family("font-family: 'Inter Tight';", "Inter Tight", "tv-x"), "font-family: 'tv-x';");
        assert!(!safe_alias("a;}b"));
    }
}
