use crate::database::Db;
use crate::downloader::DownloadManager;
use crate::error::Result;
use crate::installer::Installer;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    pub downloads: Arc<DownloadManager>,
    pub installer: Arc<Installer>,
    pub syncing: AtomicBool,
    pub default_font_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppState {
    /// Kullanıcı ayarı varsa o, yoksa uygulama verisi altındaki yönetilen font klasörü.
    pub fn font_dir(&self) -> PathBuf {
        font_dir(&self.db, &self.default_font_dir)
    }
}

pub fn font_dir(db: &Db, fallback: &std::path::Path) -> PathBuf {
    db.setting("download_dir")
        .ok()
        .flatten()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("TYPEVAULT/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(45))
        .gzip(true)
        .build()?)
}
