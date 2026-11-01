//! Google etiketlerinden çok kategorili sınıflandırma ve hazır koleksiyon kuralları.
use std::collections::HashMap;

pub const CATEGORIES: &[(&str, &str)] = &[
    ("sans-serif", "Sans Serif"),
    ("serif", "Serif"),
    ("display", "Display"),
    ("condensed", "Dar"),
    ("geometric", "Geometrik"),
    ("handwriting", "El Yazısı"),
    ("monospace", "Eş Aralıklı"),
    ("variable", "Değişken"),
    ("decorative", "Dekoratif"),
    ("futuristic", "Fütüristik"),
    ("experimental", "Deneysel"),
];

pub struct Input<'a> {
    pub family: &'a str,
    pub category: &'a str,
    pub tags: &'a HashMap<String, i64>,
    pub has_axes: bool,
    pub narrow_width_axis: bool,
    pub avg_width: Option<f64>,
}

const DECORATIVE: &[&str] = &[
    "/Theme/Art Deco", "/Theme/Art Nouveau", "/Theme/Blackletter", "/Theme/Inline", "/Theme/Medieval",
    "/Theme/Shaded", "/Theme/Tuscan", "/Theme/Woodtype", "/Theme/Wacky",
];
const EXPERIMENTAL: &[&str] = &["/Theme/Pixel", "/Theme/Blobby", "/Theme/Distressed", "/Theme/Stencil"];

pub fn classify(i: &Input) -> Vec<&'static str> {
    let tag = |name: &str, min: i64| i.tags.get(name).is_some_and(|w| *w >= min);
    let any = |names: &[&str]| names.iter().any(|n| i.tags.contains_key(*n));
    let mut out = Vec::new();
    match i.category {
        "Sans Serif" => out.push("sans-serif"),
        "Serif" => out.push("serif"),
        "Display" => out.push("display"),
        "Handwriting" => out.push("handwriting"),
        "Monospace" => out.push("monospace"),
        _ => {}
    }
    if i.has_axes {
        out.push("variable");
    }
    let lower = i.family.to_lowercase();
    let named_narrow = ["condensed", "narrow", "compressed"].iter().any(|w| lower.contains(w));
    if named_narrow || i.narrow_width_axis || i.avg_width.is_some_and(|w| w <= 4.5) {
        out.push("condensed");
    }
    if tag("/Sans/Geometric", 40) || tag("/Slab/Geometric", 40) {
        out.push("geometric");
    }
    if any(DECORATIVE) || (i.category == "Display" && tag("/Expressive/Fancy", 60)) {
        out.push("decorative");
    }
    if tag("/Expressive/Futuristic", 65) || i.tags.contains_key("/Theme/Techno") {
        out.push("futuristic");
    }
    if any(EXPERIMENTAL) || tag("/Expressive/Innovative", 70) {
        out.push("experimental");
    }
    out
}

/// Türkçe arama için kategori ve karakter anahtar sözcükleri.
pub fn keywords(categories: &[&str]) -> String {
    CATEGORIES
        .iter()
        .filter(|(id, _)| categories.contains(id))
        // "Sans Serif" dizinlenmez: bölünürse "serif" araması sans fontları da bulur.
        .map(|(id, label)| if *id == "sans-serif" { "sans".to_string() } else { format!("{id} {label}") })
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct SystemCollection {
    pub slug: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// `fonts f` üzerinde `id` ve `score` döndüren sorgu.
    pub sql: &'static str,
}


pub const SYSTEM_COLLECTIONS: &[SystemCollection] = &[
    SystemCollection {
        slug: "poster-headlines",
        name: "Afiş ve Başlık",
        description: "Uzaktan okunan, sesini yükselten başlık fontları",
        sql: "SELECT f.id, t.weight * 2 - f.popularity / 80 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id AND t.tag = '/Expressive/Loud' AND t.weight >= 50
              WHERE f.category IN ('Display', 'Sans Serif') AND f.in_catalog = 1",
    },
    SystemCollection {
        slug: "bold-heavy",
        name: "Kalın ve Ağır",
        description: "Black ve ExtraBold ağırlıkları olan güçlü aileler",
        sql: "SELECT f.id, max(v.thickness) * 40 - f.popularity / 40 AS score FROM fonts f
              JOIN font_variants v ON v.font_id = f.id
              WHERE f.in_catalog = 1 AND f.category IN ('Sans Serif', 'Display', 'Serif')
              GROUP BY f.id HAVING max(v.weight) >= 800 OR max(v.thickness) >= 8",
    },
    SystemCollection {
        slug: "futuristic-tech",
        name: "Fütüristik ve Teknoloji",
        description: "Arayüz, oyun ve teknoloji markaları için keskin formlar",
        sql: "SELECT f.id, 1000 - f.popularity / 4 AS score FROM fonts f
              JOIN font_categories c ON c.font_id = f.id AND c.category = 'futuristic'
              WHERE f.in_catalog = 1",
    },
    SystemCollection {
        slug: "editorial-luxury",
        name: "Editoryal ve Lüks",
        description: "Dergi kapağı ve lüks marka zarafeti",
        sql: "SELECT f.id, sum(t.weight) - f.popularity / 60 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id
                AND t.tag IN ('/Expressive/Sophisticated', '/Serif/Didone', '/Serif/Fat Face', '/Serif/Modern', '/Expressive/Fancy')
              WHERE f.in_catalog = 1 AND f.category IN ('Serif', 'Display')
              GROUP BY f.id",
    },
    SystemCollection {
        slug: "minimal-modern",
        name: "Minimal ve Modern",
        description: "Sakin, temiz ve zamansız sans aileleri",
        sql: "SELECT f.id, sum(t.weight) - f.popularity / 30 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id
                AND t.tag IN ('/Sans/Neo Grotesque', '/Sans/Geometric', '/Sans/Superellipse', '/Expressive/Calm')
              WHERE f.in_catalog = 1 AND f.category = 'Sans Serif'
              GROUP BY f.id",
    },
    SystemCollection {
        slug: "condensed-typography",
        name: "Dar Tipografi",
        description: "Az yerde çok şey söyleyen dar kesimler",
        sql: "SELECT f.id, 1000 - f.popularity / 4 AS score FROM fonts f
              JOIN font_categories c ON c.font_id = f.id AND c.category = 'condensed'
              WHERE f.in_catalog = 1",
    },
    SystemCollection {
        slug: "social-media",
        name: "Sosyal Medya Tasarımı",
        description: "Akışta göze çarpan enerjik ve dost canlısı fontlar",
        sql: "SELECT f.id, sum(t.weight) - f.popularity / 30 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id
                AND t.tag IN ('/Expressive/Playful', '/Expressive/Happy', '/Expressive/Excited', '/Expressive/Active')
              WHERE f.in_catalog = 1 AND f.category IN ('Sans Serif', 'Display', 'Handwriting')
              GROUP BY f.id",
    },
    SystemCollection {
        slug: "corporate-branding",
        name: "Kurumsal Kimlik",
        description: "Güven veren, çok ağırlıklı kurumsal aileler",
        sql: "SELECT f.id, sum(t.weight) - f.popularity / 30 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id AND t.tag IN ('/Expressive/Business', '/Expressive/Competent')
              WHERE f.in_catalog = 1 AND f.category IN ('Sans Serif', 'Serif')
                AND (SELECT count(*) FROM font_variants v WHERE v.font_id = f.id) >= 4
              GROUP BY f.id",
    },
    SystemCollection {
        slug: "food-restaurant",
        name: "Yiyecek ve Restoran",
        description: "Menü, ambalaj ve vitrin için sıcak karakterler",
        sql: "SELECT f.id, sum(t.weight) - f.popularity / 40 AS score FROM fonts f
              JOIN font_tags t ON t.font_id = f.id
                AND t.tag IN ('/Script/Handwritten', '/Theme/Brush', '/Script/Informal', '/Theme/Woodtype', '/Script/Upright Script')
              WHERE f.in_catalog = 1
              GROUP BY f.id",
    },
    SystemCollection {
        slug: "turkish-friendly",
        name: "Türkçe Dostu",
        description: "Tüm Türkçe karakterleri doğrulanmış fontlar",
        sql: "SELECT f.id, 100000 - f.popularity AS score FROM fonts f
              WHERE f.in_catalog = 1 AND f.turkish_status = 'full'",
    },
];

/// Küratörlü koleksiyonların boyut sınırı; Türkçe Dostu sınırsızdır.
pub fn collection_limit(slug: &str) -> i64 {
    if slug == "turkish-friendly" { 100_000 } else { 200 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(items: &[(&str, i64)]) -> HashMap<String, i64> {
        items.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn base<'a>(family: &'a str, category: &'a str, t: &'a HashMap<String, i64>) -> Input<'a> {
        Input { family, category, tags: t, has_axes: false, narrow_width_axis: false, avg_width: None }
    }

    #[test]
    fn multi_category() {
        let t = tags(&[("/Sans/Geometric", 80), ("/Expressive/Futuristic", 90)]);
        let got = classify(&Input { has_axes: true, ..base("Orbitron", "Sans Serif", &t) });
        for c in ["sans-serif", "geometric", "futuristic", "variable"] {
            assert!(got.contains(&c), "{c}");
        }
    }

    #[test]
    fn condensed_from_name_axis_or_width() {
        let t = tags(&[]);
        assert!(classify(&base("Roboto Condensed", "Sans Serif", &t)).contains(&"condensed"));
        assert!(classify(&Input { avg_width: Some(3.0), ..base("Oswald", "Sans Serif", &t) }).contains(&"condensed"));
        assert!(!classify(&Input { avg_width: Some(7.0), ..base("Inter", "Sans Serif", &t) }).contains(&"condensed"));
    }

    #[test]
    fn serif_keyword_stays_out_of_sans() {
        assert!(!keywords(&["sans-serif"]).contains("erif"));
        assert!(keywords(&["serif"]).contains("serif"));
    }

    #[test]
    fn weak_tags_do_not_qualify() {
        let t = tags(&[("/Expressive/Futuristic", 20)]);
        assert!(!classify(&base("Lora", "Serif", &t)).contains(&"futuristic"));
    }
}
