//! Türkçe glif denetimi. "Tam" yalnızca gerekli tüm glifler varsa verilir.

pub const REQUIRED: [char; 14] = ['ç', 'Ç', 'ğ', 'Ğ', 'ı', 'İ', 'i', 'I', 'ö', 'Ö', 'ş', 'Ş', 'ü', 'Ü'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Support {
    pub status: &'static str,
    pub missing: String,
}

pub fn evaluate(has: impl Fn(char) -> bool) -> Support {
    let missing: String = REQUIRED.iter().copied().filter(|c| !has(*c)).collect();
    let present_special = REQUIRED.iter().filter(|c| !matches!(c, 'i' | 'I') && has(**c)).count();
    let status = if missing.is_empty() {
        "full"
    } else if present_special == 0 {
        "none"
    } else {
        "partial"
    };
    Support { status, missing }
}

/// Google'ın "0,13,32-126,..." biçimindeki kapsam dizisini arar.
pub fn ranges_contain(ranges: &str, codepoint: u32) -> bool {
    ranges.split(',').any(|part| {
        let part = part.trim();
        match part.split_once('-') {
            Some((a, b)) => matches!((a.parse::<u32>(), b.parse::<u32>()), (Ok(a), Ok(b)) if (a..=b).contains(&codepoint)),
            None => part.parse::<u32>() == Ok(codepoint),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_requires_every_glyph() {
        assert_eq!(evaluate(|_| true).status, "full");
        let partial = evaluate(|c| c != 'İ');
        assert_eq!(partial.status, "partial");
        assert_eq!(partial.missing, "İ");
    }

    #[test]
    fn none_when_only_ascii() {
        let s = evaluate(|c| c.is_ascii());
        assert_eq!(s.status, "none");
        assert_eq!(s.missing.chars().count(), 12);
    }

    #[test]
    fn parses_ranges() {
        let r = "0,13,32-126,256-304,306-337";
        assert!(ranges_contain(r, 'A' as u32));
        assert!(ranges_contain(r, 304));
        assert!(!ranges_contain(r, 305));
        assert!(ranges_contain(r, 13));
    }

    #[test]
    fn real_world_coverage_detects_dotless_i() {
        // Abril Fatface kapsamı 305'i latin alt kümesinde tutar; yalnız latin-ext yetmez.
        let latin_ext = "32,160,256-304,306-337,340-383";
        let s = evaluate(|c| ranges_contain(latin_ext, c as u32));
        assert_eq!(s.status, "partial");
        assert!(s.missing.contains('ı'));
    }
}
