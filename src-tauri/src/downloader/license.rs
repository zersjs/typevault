//! Manifestteki lisans metninden lisans türünü belirler.
use crate::error::{AppError, Result};
use crate::metadata::google::ManifestFile;

pub fn detect(files: &[ManifestFile]) -> Option<(&'static str, &ManifestFile)> {
    files.iter().find_map(|f| {
        let name = f.filename.to_lowercase();
        let text = &f.contents;
        if name.contains("ofl") || text.contains("SIL OPEN FONT LICENSE") || text.contains("SIL Open Font License") {
            Some(("ofl", f))
        } else if name.contains("ufl") || text.contains("UBUNTU FONT LICENCE") {
            Some(("ufl", f))
        } else if (name.contains("license") || name.contains("apache")) && text.contains("Apache License") {
            Some(("apache2", f))
        } else {
            None
        }
    })
}

pub fn reconcile(catalog: Option<&str>, embedded: Option<&str>) -> Result<String> {
    match (catalog, embedded) {
        (Some(a), Some(b)) if a != b => {
            Err(AppError::License(format!("katalog '{a}', dosya '{b}' diyor")))
        }
        (_, Some(code)) | (Some(code), None) => Ok(code.to_string()),
        (None, None) => Err(AppError::License("lisans bilgisi bulunamadı".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, contents: &str) -> ManifestFile {
        ManifestFile { filename: name.into(), contents: contents.into() }
    }

    #[test]
    fn detects_licenses() {
        assert_eq!(detect(&[file("OFL.txt", "x")]).unwrap().0, "ofl");
        assert_eq!(detect(&[file("README.txt", "x"), file("LICENSE.txt", "Apache License\nVersion 2.0")]).unwrap().0, "apache2");
        assert_eq!(detect(&[file("UFL.txt", "x")]).unwrap().0, "ufl");
        assert!(detect(&[file("README.txt", "hello")]).is_none());
    }

    #[test]
    fn conflicts_and_unknown_are_refused() {
        assert_eq!(reconcile(Some("ofl"), Some("ofl")).unwrap(), "ofl");
        assert_eq!(reconcile(None, Some("apache2")).unwrap(), "apache2");
        assert!(reconcile(Some("ofl"), Some("apache2")).is_err());
        assert!(reconcile(None, None).is_err());
    }
}
