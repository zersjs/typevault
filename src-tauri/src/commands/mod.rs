//! Ön yüze açılan tipli Tauri komutları.
pub mod catalog;
pub mod collections;
pub mod library;
pub mod transfer;

use crate::error::{AppError, Result};
use crate::security::check_font_id;

pub fn check_ids(ids: &[String]) -> Result<()> {
    if ids.len() > 5000 {
        return Err(AppError::Invalid("Tek işlemde en fazla 5000 font seçilebilir".into()));
    }
    ids.iter().try_for_each(|id| check_font_id(id).map(|_| ()))
}
