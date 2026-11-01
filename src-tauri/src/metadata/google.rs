//! Google Fonts kaynakları: Developer API (anahtar varsa) ve fonts.google.com meta verisi.
use crate::error::{AppError, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use url::Url;

const METADATA_URL: &str = "https://fonts.google.com/metadata/fonts";
const TAGS_URL: &str = "https://raw.githubusercontent.com/google/fonts/main/tags/all/families.csv";
const DEVELOPER_API_URL: &str = "https://www.googleapis.com/webfonts/v1/webfonts";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResponse {
    pub axis_registry: Vec<AxisInfo>,
    pub family_metadata_list: Vec<Family>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxisInfo {
    pub tag: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Family {
    pub family: String,
    pub category: String,
    #[serde(default)]
    pub subsets: Vec<String>,
    #[serde(default)]
    pub fonts: BTreeMap<String, Variant>,
    #[serde(default)]
    pub axes: Vec<Axis>,
    #[serde(default)]
    pub designers: Vec<String>,
    pub last_modified: Option<String>,
    pub date_added: Option<String>,
    pub popularity: Option<i64>,
    pub trending: Option<i64>,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub is_noto: bool,
    #[serde(default)]
    pub color_capabilities: Vec<String>,
    #[serde(skip)]
    pub api_files: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Variant {
    pub thickness: Option<i64>,
    pub width: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Axis {
    pub tag: String,
    pub min: f64,
    pub max: f64,
    pub default_value: f64,
}

#[derive(Debug, Deserialize)]
pub struct FamilyDetail {
    #[serde(default)]
    pub coverage: HashMap<String, String>,
    pub license: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadManifest {
    pub manifest: Manifest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(default)]
    pub file_refs: Vec<FileRef>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestFile {
    pub filename: String,
    pub contents: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileRef {
    pub filename: String,
    pub url: String,
}

/// Google'ın XSSI korumalı yanıtlarındaki `)]}'` önekini temizler.
pub fn strip_xssi(body: &str) -> &str {
    body.strip_prefix(")]}'").unwrap_or(body).trim_start()
}

async fn get_text(http: &reqwest::Client, url: &str) -> Result<String> {
    let res = http.get(url).send().await?;
    if !res.status().is_success() {
        return Err(AppError::Status(res.status().as_u16()));
    }
    Ok(res.text().await?)
}

pub async fn fetch_catalog(http: &reqwest::Client) -> Result<CatalogResponse> {
    let body = get_text(http, METADATA_URL).await?;
    Ok(serde_json::from_str(strip_xssi(&body))?)
}

fn family_query(base: &str, family: &str) -> String {
    let mut url = Url::parse(base).expect("sabit adres");
    url.query_pairs_mut().append_pair("family", family);
    url.to_string()
}

pub async fn fetch_detail(http: &reqwest::Client, family: &str) -> Result<FamilyDetail> {
    let mut url = Url::parse(METADATA_URL).expect("sabit adres");
    url.path_segments_mut().expect("https").push(family);
    let body = get_text(http, url.as_str()).await?;
    Ok(serde_json::from_str(strip_xssi(&body))?)
}

pub async fn fetch_manifest(http: &reqwest::Client, family: &str) -> Result<Manifest> {
    let body = get_text(http, &family_query("https://fonts.google.com/download/list", family)).await?;
    let parsed: DownloadManifest = serde_json::from_str(strip_xssi(&body))?;
    Ok(parsed.manifest)
}

/// Google'ın `tags/all/families.csv` dosyası: aile → etiket → ağırlık.
pub async fn fetch_tags(http: &reqwest::Client) -> Result<HashMap<String, HashMap<String, i64>>> {
    parse_tags(&get_text(http, TAGS_URL).await?)
}

pub fn parse_tags(body: &str) -> Result<HashMap<String, HashMap<String, i64>>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(body.as_bytes());
    let mut out: HashMap<String, HashMap<String, i64>> = HashMap::new();
    for record in reader.records().flatten() {
        // Biçim: Aile, (eksen), /Grup/Etiket, ağırlık — eksene özgü satırlar atlanır.
        let (Some(family), Some(tag), Some(weight)) = (record.get(0), record.get(2), record.get(3)) else {
            continue;
        };
        if !tag.starts_with('/') || !record.get(1).unwrap_or_default().is_empty() {
            continue;
        }
        if let Ok(weight) = weight.trim().parse::<f64>() {
            out.entry(family.to_string()).or_default().insert(tag.to_string(), weight as i64);
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    items: Vec<ApiFamily>,
}

#[derive(Debug, Deserialize)]
struct ApiFamily {
    family: String,
    category: String,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default)]
    subsets: Vec<String>,
    #[serde(rename = "lastModified")]
    last_modified: Option<String>,
    #[serde(default)]
    files: BTreeMap<String, String>,
    #[serde(default)]
    axes: Vec<ApiAxis>,
}

#[derive(Debug, Deserialize)]
struct ApiAxis {
    tag: String,
    start: f64,
    end: f64,
}

/// Resmî Developer API. Anahtar yanlışsa açık bir hata döner.
pub async fn fetch_developer_api(http: &reqwest::Client, key: &str) -> Result<Vec<Family>> {
    let mut url = Url::parse(DEVELOPER_API_URL).expect("sabit adres");
    url.query_pairs_mut().append_pair("key", key).append_pair("sort", "popularity").append_pair("capability", "VF");
    let res = http.get(url).send().await?;
    match res.status().as_u16() {
        200 => {}
        400 | 403 => return Err(AppError::Invalid("Google Fonts API anahtarı reddedildi".into())),
        code => return Err(AppError::Status(code)),
    }
    let parsed: ApiResponse = res.json().await?;
    Ok(parsed.items.into_iter().map(ApiFamily::into_family).collect())
}

impl ApiFamily {
    fn into_family(self) -> Family {
        let category = match self.category.as_str() {
            "sans-serif" => "Sans Serif",
            "serif" => "Serif",
            "display" => "Display",
            "handwriting" => "Handwriting",
            "monospace" => "Monospace",
            other => other,
        }
        .to_string();
        let fonts = self
            .variants
            .iter()
            .map(|v| {
                let key = match v.as_str() {
                    "regular" => "400".to_string(),
                    "italic" => "400i".to_string(),
                    other => other.replace("italic", "i"),
                };
                (key, Variant { thickness: None, width: None })
            })
            .collect();
        let axes = self
            .axes
            .into_iter()
            .map(|a| Axis { default_value: a.start.max(400f64.min(a.end)), tag: a.tag, min: a.start, max: a.end })
            .collect();
        Family {
            family: self.family,
            category,
            subsets: self.subsets,
            fonts,
            axes,
            designers: Vec::new(),
            last_modified: self.last_modified,
            date_added: None,
            popularity: None,
            trending: None,
            size: 0,
            is_noto: false,
            color_capabilities: Vec::new(),
            api_files: Some(self.files),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_prefix() {
        assert_eq!(strip_xssi(")]}'\n{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_xssi("{\"a\":1}"), "{\"a\":1}");
    }

    #[test]
    fn parses_tags_and_skips_axis_rows() {
        let csv = "ABeeZee,,/Expressive/Active,10\nAnybody,\"wdth@50,wght@900\",/Expressive/Loud,99\nInter,,/Sans/Neo Grotesque,80.5\n";
        let tags = parse_tags(csv).unwrap();
        assert_eq!(tags["ABeeZee"]["/Expressive/Active"], 10);
        assert_eq!(tags["Inter"]["/Sans/Neo Grotesque"], 80);
        assert!(!tags.contains_key("Anybody"));
    }

    #[test]
    fn maps_developer_api_shape() {
        let json = r#"{"items":[{"family":"Inter","category":"sans-serif","variants":["regular","italic","700","700italic"],
          "subsets":["latin","latin-ext"],"lastModified":"2026-01-01","files":{"regular":"https://fonts.gstatic.com/s/inter/a.ttf"},
          "axes":[{"tag":"wght","start":100,"end":900}]}]}"#;
        let parsed: ApiResponse = serde_json::from_str(json).unwrap();
        let family = parsed.items.into_iter().next().unwrap().into_family();
        assert_eq!(family.category, "Sans Serif");
        assert!(family.fonts.contains_key("700i"));
        assert!(family.fonts.contains_key("400"));
        assert_eq!(family.axes[0].default_value, 400.0);
        assert_eq!(family.api_files.unwrap()["regular"], "https://fonts.gstatic.com/s/inter/a.ttf");
    }

    #[test]
    fn manifest_shape() {
        let json = r#")]}'
{"zipName":"A.zip","manifest":{"files":[{"filename":"OFL.txt","contents":"x"}],"fileRefs":[{"filename":"A.ttf","url":"https://fonts.gstatic.com/a.ttf","date":{"seconds":1}}]}}"#;
        let m: DownloadManifest = serde_json::from_str(strip_xssi(json)).unwrap();
        assert_eq!(m.manifest.file_refs[0].filename, "A.ttf");
    }
}
