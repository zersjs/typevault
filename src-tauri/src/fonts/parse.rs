//! İndirilen dosyaların ikili düzeyde doğrulanması.
use super::turkish::{self, Support};
use crate::error::{AppError, Result};
use ttf_parser::{name_id, Face};

#[derive(Debug, Clone)]
pub struct FontInfo {
    pub format: &'static str,
    pub postscript_name: String,
    pub weight: u16,
    pub italic: bool,
    pub is_variable: bool,
    pub turkish: Support,
}

pub fn sniff_format(data: &[u8]) -> Option<&'static str> {
    match data.get(..4)? {
        [0, 1, 0, 0] | b"true" => Some("ttf"),
        b"OTTO" => Some("otf"),
        _ => None,
    }
}

/// Tablo dizinindeki her tablo dosyanın içinde olmalıdır; kesik indirmeleri yakalar.
fn check_table_bounds(data: &[u8]) -> Result<()> {
    let u16_at = |i: usize| data.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let truncated = || AppError::Font("dosya eksik veya kesik".into());
    let tables = u16_at(4).ok_or_else(truncated)?;
    for index in 0..tables {
        let record = 12 + index * 16;
        let offset = u32_at(record + 8).ok_or_else(truncated)?;
        let length = u32_at(record + 12).ok_or_else(truncated)?;
        if offset.saturating_add(length) > data.len() {
            return Err(truncated());
        }
    }
    Ok(())
}

pub fn inspect(data: &[u8]) -> Result<FontInfo> {
    let format = sniff_format(data).ok_or_else(|| {
        let looks_web = matches!(data.get(..4), Some(b"wOFF") | Some(b"wOF2"));
        AppError::Font(if looks_web {
            "WOFF dosyaları masaüstü fontu olarak kurulamaz".into()
        } else {
            "TTF/OTF imzası bulunamadı".into()
        })
    })?;
    check_table_bounds(data)?;
    let face = Face::parse(data, 0).map_err(|e| AppError::Font(format!("çözümlenemedi ({e})")))?;
    if face.number_of_glyphs() < 2 {
        return Err(AppError::Font("glif içermiyor".into()));
    }
    let name = |id: u16| {
        face.names()
            .into_iter()
            .filter(|n| n.name_id == id && n.is_unicode())
            .find_map(|n| n.to_string())
    };
    name(name_id::FAMILY).ok_or_else(|| AppError::Font("aile adı yok".into()))?;
    let postscript_name = name(name_id::POST_SCRIPT_NAME)
        .ok_or_else(|| AppError::Font("PostScript adı yok".into()))?;
    let turkish = turkish::evaluate(|c| face.glyph_index(c).is_some_and(|g| g.0 != 0));
    Ok(FontInfo {
        format,
        postscript_name,
        weight: face.weight().to_number(),
        italic: face.is_italic(),
        is_variable: face.is_variable(),
        turkish,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_fonts() {
        assert!(inspect(b"<html>not found</html>").is_err());
        assert!(inspect(b"wOF2aaaaaaaaaaaaaaaa").unwrap_err().to_string().contains("WOFF"));
        assert!(inspect(&[0, 1, 0, 0, 9, 9, 9]).is_err());
    }

    #[test]
    fn rejects_truncated_files() {
        let data = std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf").unwrap();
        let cut = &data[..data.len() - data.len() / 10];
        assert!(inspect(cut).unwrap_err().to_string().contains("kesik"));
    }

    #[test]
    fn reads_system_font() {
        let data = std::fs::read("/System/Library/Fonts/Supplemental/Arial.ttf").unwrap();
        let info = inspect(&data).unwrap();
        assert_eq!(info.postscript_name, "ArialMT");
        assert_eq!(info.turkish.status, "full");
    }
}
