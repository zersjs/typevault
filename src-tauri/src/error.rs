use serde::{Serialize, Serializer};

pub type Result<T> = std::result::Result<T, AppError>;

/// Tüm hatalar kullanıcıya gösterilecek Türkçe bir iletiye dönüşür.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Veritabanı hatası: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Veritabanı bağlantı hatası: {0}")]
    Pool(#[from] r2d2::Error),
    #[error("Dosya hatası: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ağ hatası: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Veri çözümlenemedi: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("Bulunamadı: {0}")]
    NotFound(String),
    #[error("Font dosyası geçersiz: {0}")]
    Font(String),
    #[error("Lisans doğrulanamadı: {0}")]
    License(String),
    #[error("Kurulum başarısız: {0}")]
    Install(String),
    #[error("Sunucu {0} durum kodu döndürdü")]
    Status(u16),
    #[error("Bağlantı dosya tamamlanmadan kesildi")]
    Incomplete,
}

impl AppError {
    /// Yeniden denemenin anlamlı olduğu geçici hatalar.
    pub fn is_transient(&self) -> bool {
        match self {
            AppError::Http(_) | AppError::Incomplete => true,
            AppError::Status(code) => *code == 429 || *code >= 500,
            AppError::Io(_) => true,
            _ => false,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
