//! Ön yüzden veya uzak kaynaktan gelen girdiler için doğrulama.
use crate::error::{AppError, Result};
use std::path::{Component, Path, PathBuf};
use url::Url;

/// Font dosyalarının indirilebileceği sunucular.
const TRUSTED_HOSTS: &[&str] = &["fonts.gstatic.com", "fonts.googleapis.com", "fonts.google.com"];

/// Tek bir font dosyası için üst sınır. Google Fonts'taki en büyük CJK aileleri bunun altındadır.
pub const MAX_FONT_BYTES: u64 = 128 * 1024 * 1024;

pub fn check_trusted_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| AppError::Invalid(format!("Geçersiz adres: {raw}")))?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https" || !TRUSTED_HOSTS.contains(&host) {
        return Err(AppError::Invalid(format!("Güvenilmeyen kaynak: {host}")));
    }
    Ok(url)
}

/// Kataloğa ait kararlı kimlik: `gf:roboto-slab`.
pub fn check_font_id(id: &str) -> Result<&str> {
    let valid = id.len() <= 128
        && id.starts_with("gf:")
        && id[3..].chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if valid && id.len() > 3 {
        Ok(id)
    } else {
        Err(AppError::Invalid(format!("Geçersiz font kimliği: {id}")))
    }
}

pub fn slugify(family: &str) -> String {
    let mut out = String::with_capacity(family.len());
    for c in family.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Uzak manifestten gelen dosya adını klasörsüz, güvenli bir ada indirger.
pub fn sanitize_filename(name: &str) -> Result<String> {
    let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '[' | ']' | ',' | ' '))
        .collect();
    if clean.is_empty() || clean.starts_with('.') || clean.len() > 200 {
        return Err(AppError::Invalid(format!("Geçersiz dosya adı: {name}")));
    }
    Ok(clean)
}

/// `root` dışına çıkamayan birleştirme; `..` ve mutlak yolları reddeder.
pub fn safe_join(root: &Path, rel: &str) -> Result<PathBuf> {
    let rel_path = Path::new(rel);
    for part in rel_path.components() {
        if !matches!(part, Component::Normal(_)) {
            return Err(AppError::Invalid(format!("Güvensiz yol: {rel}")));
        }
    }
    Ok(root.join(rel_path))
}

pub fn is_inside(root: &Path, candidate: &Path) -> bool {
    let has_parent_ref = candidate.components().any(|c| matches!(c, Component::ParentDir));
    !has_parent_ref && candidate.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_urls() {
        assert!(check_trusted_url("https://fonts.gstatic.com/s/a.ttf").is_ok());
        assert!(check_trusted_url("http://fonts.gstatic.com/s/a.ttf").is_err());
        assert!(check_trusted_url("https://fonts.gstatic.com.evil.io/a.ttf").is_err());
        assert!(check_trusted_url("file:///etc/passwd").is_err());
    }

    #[test]
    fn font_id_format() {
        assert!(check_font_id("gf:roboto-slab").is_ok());
        assert!(check_font_id("gf:").is_err());
        assert!(check_font_id("gf:../../etc").is_err());
        assert!(check_font_id("roboto").is_err());
    }

    #[test]
    fn slugs_are_stable() {
        assert_eq!(slugify("Roboto Slab"), "roboto-slab");
        assert_eq!(slugify("M PLUS 1p"), "m-plus-1p");
        assert_eq!(slugify("Playwrite AR Guides"), "playwrite-ar-guides");
    }

    #[test]
    fn filenames_lose_directories() {
        assert_eq!(sanitize_filename("static/Roboto-Thin.ttf").unwrap(), "Roboto-Thin.ttf");
        assert_eq!(sanitize_filename("../../evil.ttf").unwrap(), "evil.ttf");
        assert_eq!(sanitize_filename("Roboto[wdth,wght].ttf").unwrap(), "Roboto[wdth,wght].ttf");
        assert!(sanitize_filename("..").is_err());
    }

    #[test]
    fn safe_join_blocks_traversal() {
        let root = Path::new("/a/b");
        assert!(safe_join(root, "c/d.ttf").is_ok());
        assert!(safe_join(root, "../d.ttf").is_err());
        assert!(safe_join(root, "/etc/passwd").is_err());
        assert!(is_inside(root, Path::new("/a/b/c")));
        assert!(!is_inside(root, Path::new("/a/b/../x")));
        assert!(!is_inside(root, Path::new("/a/bc")));
    }
}
