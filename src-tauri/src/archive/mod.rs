//! Kaynağın ZIP sunduğu durumlar için güvenli font çıkarma.
use crate::error::{AppError, Result};
use crate::security::{sanitize_filename, MAX_FONT_BYTES};
use std::io::{Cursor, Read};

pub struct Extracted {
    pub filename: String,
    pub data: Vec<u8>,
}

pub fn is_zip(data: &[u8]) -> bool {
    data.starts_with(b"PK\x03\x04")
}

/// Yalnız TTF/OTF girdilerini belleğe alır; arşivdeki yollar asla diske uygulanmaz.
pub fn extract_fonts(data: &[u8]) -> Result<Vec<Extracted>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data))
        .map_err(|e| AppError::Font(format!("arşiv açılamadı: {e}")))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| AppError::Font(format!("arşiv girdisi okunamadı: {e}")))?;
        let lower = entry.name().to_lowercase();
        let is_font = lower.ends_with(".ttf") || lower.ends_with(".otf");
        if !entry.is_file() || !is_font || lower.starts_with("__macosx/") {
            continue;
        }
        if entry.size() > MAX_FONT_BYTES {
            return Err(AppError::Font(format!("{} çok büyük", entry.name())));
        }
        let filename = sanitize_filename(entry.name())?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.take(MAX_FONT_BYTES + 1).read_to_end(&mut buf)?;
        if buf.len() as u64 > MAX_FONT_BYTES {
            return Err(AppError::Font("açılan boyut sınırı aşıldı".into()));
        }
        out.push(Extracted { filename, data: buf });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extracts_only_fonts_and_flattens_paths() {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("../../evil/A.ttf", opts).unwrap();
            zip.write_all(b"font").unwrap();
            zip.start_file("README.txt", opts).unwrap();
            zip.write_all(b"hi").unwrap();
            zip.start_file("__MACOSX/._A.ttf", opts).unwrap();
            zip.write_all(b"junk").unwrap();
            zip.finish().unwrap();
        }
        assert!(is_zip(&buf));
        let got = extract_fonts(&buf).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].filename, "A.ttf");
    }
}
