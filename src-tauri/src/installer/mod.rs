//! Kalıcı kurulum akışı: doğrula → ~/Library/Fonts/TYPEVAULT içine kopyala → CoreText'e kaydet → sistemden teyit al.
pub mod coretext;

use crate::database::Db;
use crate::downloader::mark_batch;
use crate::error::{AppError, Result};
use crate::fonts::parse;
use crate::security::{is_inside, safe_join};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallEvent {
    pub font_id: String,
    pub family: String,
    pub state: &'static str,
    pub message: Option<String>,
}

enum Task {
    Install { font_id: String, batch: Option<i64> },
}

pub struct Installer {
    db: Db,
    app: AppHandle,
    root: PathBuf,
    tx: Sender<Task>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReport {
    pub installed: usize,
    pub external: usize,
    pub failed: Vec<String>,
}

struct FileRow {
    id: i64,
    filename: String,
    local_path: String,
    sha256: String,
    postscript_name: String,
}

pub fn user_font_root() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").ok_or_else(|| AppError::Install("HOME tanımlı değil".into()))?;
    Ok(PathBuf::from(home).join("Library/Fonts/TYPEVAULT"))
}

impl Installer {
    pub fn new(db: Db, app: AppHandle, root: PathBuf) -> Arc<Self> {
        let (tx, rx) = channel::<Task>();
        let installer = Arc::new(Self { db, app, root, tx });
        let worker = installer.clone();
        std::thread::Builder::new()
            .name("typevault-installer".into())
            .spawn(move || {
                for task in rx {
                    let Task::Install { font_id, batch } = task;
                    worker.handle(&font_id, batch);
                }
            })
            .expect("kurulum iş parçacığı");
        installer
    }

    pub fn enqueue(&self, font_id: String, batch: Option<i64>) {
        let _ = self.tx.send(Task::Install { font_id, batch });
    }

    fn family(&self, font_id: &str) -> String {
        self.db
            .conn()
            .ok()
            .and_then(|c| c.query_row("SELECT family FROM fonts WHERE id=?1", [font_id], |r| r.get(0)).ok())
            .unwrap_or_else(|| font_id.to_string())
    }

    fn emit(&self, font_id: &str, state: &'static str, message: Option<String>) {
        let event = InstallEvent { font_id: font_id.into(), family: self.family(font_id), state, message };
        let _ = self.app.emit("install://progress", event);
        let _ = self.app.emit("library://changed", ());
    }

    fn handle(&self, font_id: &str, batch: Option<i64>) {
        self.emit(font_id, "installing", None);
        match install_font(&self.db, &self.root, font_id) {
            Ok(report) if report.failed.is_empty() => {
                if let Some(batch) = batch {
                    let _ = mark_batch(&self.db, batch, font_id, "done", None);
                }
                let message = (report.installed == 0 && report.external > 0)
                    .then(|| "Bu font sistemde başka bir kaynaktan zaten kurulu".to_string());
                self.emit(font_id, "installed", message);
            }
            Ok(report) => {
                let message = report.failed.join("; ");
                if let Some(batch) = batch {
                    let _ = mark_batch(&self.db, batch, font_id, "failed", Some(&message));
                }
                self.emit(font_id, "failed", Some(message));
            }
            Err(e) => {
                let message = e.to_string();
                let _ = self.db.conn().and_then(|c| {
                    Ok(c.execute(
                        "UPDATE fonts SET local_status='install_failed', local_error=?2 WHERE id=?1",
                        params![font_id, message],
                    )?)
                });
                if let Some(batch) = batch {
                    let _ = mark_batch(&self.db, batch, font_id, "failed", Some(&message));
                }
                self.emit(font_id, "failed", Some(message));
            }
        }
    }

    pub fn uninstall(&self, font_id: &str) -> Result<usize> {
        let removed = uninstall_font(&self.db, &self.root, font_id)?;
        let _ = self.app.emit("library://changed", ());
        Ok(removed)
    }

    pub fn reconcile(&self) -> Result<()> {
        reconcile(&self.db, &self.root)?;
        self.resume_batches()
    }

    /// Bellekteki kurulum kuyruğu kapanışta kaybolur; bekleyen toplu işler veritabanından sürdürülür.
    fn resume_batches(&self) -> Result<()> {
        let pending: Vec<(i64, String, bool)> = {
            let conn = self.db.conn()?;
            let mut stmt = conn.prepare(
                "SELECT s.batch_id, s.font_id, b.install_after FROM batch_fonts s
                 JOIN batches b ON b.id = s.batch_id
                 WHERE s.state='pending' AND NOT EXISTS (SELECT 1 FROM downloads d
                   WHERE d.font_id=s.font_id AND d.status IN ('queued','downloading','paused'))",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for (batch, font, install) in pending {
            let downloaded: bool = self.db.conn()?.query_row(
                "SELECT EXISTS (SELECT 1 FROM font_files WHERE font_id=?1 AND local_path IS NOT NULL)",
                [&font],
                |r| r.get(0),
            )?;
            if downloaded && install {
                self.enqueue(font, Some(batch));
            } else if downloaded {
                mark_batch(&self.db, batch, &font, "done", None)?;
            } else {
                mark_batch(&self.db, batch, &font, "failed", Some("İşlem yarıda kaldı; yeniden deneyin"))?;
            }
        }
        Ok(())
    }
}

fn wait_until_registered(path: &Path) -> bool {
    for _ in 0..20 {
        if coretext::is_registered(path) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

fn license_allows(db: &Db, font_id: &str) -> Result<()> {
    let allowed: Option<bool> = db
        .conn()?
        .query_row(
            "SELECT l.allows_install FROM fonts f JOIN licenses l ON l.id = f.license_id WHERE f.id=?1",
            [font_id],
            |r| r.get(0),
        )
        .optional()?;
    match allowed {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::License("bu lisans türü otomatik kurulumu desteklemiyor".into())),
        None => Err(AppError::License("font için doğrulanmış lisans yok".into())),
    }
}

fn load_files(db: &Db, font_id: &str) -> Result<(String, Vec<FileRow>)> {
    let conn = db.conn()?;
    let slug: String = conn
        .query_row("SELECT slug FROM fonts WHERE id=?1", [font_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| AppError::NotFound(font_id.to_string()))?;
    let mut stmt = conn.prepare(
        "SELECT id, filename, local_path, sha256, postscript_name FROM font_files
         WHERE font_id=?1 AND local_path IS NOT NULL AND sha256 IS NOT NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([font_id], |r| {
            Ok(FileRow {
                id: r.get(0)?,
                filename: r.get(1)?,
                local_path: r.get(2)?,
                sha256: r.get(3)?,
                postscript_name: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok((slug, rows))
}

fn record(db: &Db, font_id: &str, file: &FileRow, owner: &str, status: &str, path: Option<&Path>, error: Option<&str>) -> Result<()> {
    db.conn()?.execute(
        "INSERT INTO installations (font_id, file_id, owner, status, installed_path, postscript_name, sha256, error)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
         ON CONFLICT(file_id) DO UPDATE SET owner=excluded.owner, status=excluded.status,
           installed_path=excluded.installed_path, sha256=excluded.sha256, error=excluded.error,
           installed_at=datetime('now')",
        params![
            font_id,
            file.id,
            owner,
            status,
            path.map(|p| p.to_string_lossy().to_string()),
            file.postscript_name,
            file.sha256,
            error
        ],
    )?;
    Ok(())
}

/// Eksik veya bozuk kopya atılır; böylece "Yeniden dene" aynı dosyayla değil yeni indirmeyle sürer.
fn validate_file(db: &Db, file: &FileRow) -> Result<Vec<u8>> {
    let checked = std::fs::read(&file.local_path)
        .map_err(|_| AppError::Font(format!("{} bulunamadı", file.filename)))
        .and_then(|data| {
            if hex::encode(Sha256::digest(&data)) != file.sha256 {
                return Err(AppError::Font(format!("{} bozulmuş; kurulum durduruldu", file.filename)));
            }
            parse::inspect(&data)?;
            Ok(data)
        });
    if checked.is_err() {
        discard(db, file)?;
    }
    checked
}

fn discard(db: &Db, file: &FileRow) -> Result<()> {
    let _ = std::fs::remove_file(&file.local_path);
    db.conn()?.execute("UPDATE font_files SET local_path=NULL, sha256=NULL WHERE id=?1", [file.id])?;
    Ok(())
}

fn install_one(db: &Db, root: &Path, slug: &str, font_id: &str, file: &FileRow, seen: &mut HashSet<String>) -> Result<bool> {
    let data = validate_file(db, file)?;
    let names = coretext::describe(Path::new(&file.local_path))?;
    let postscript = names.first().cloned().unwrap_or_else(|| file.postscript_name.clone());
    if names.is_empty() {
        discard(db, file)?;
        return Err(AppError::Font(format!("{} macOS tarafından tanınmadı", file.filename)));
    }
    let target = safe_join(&root.join(slug), &file.filename)?;
    // Aynı PostScript adı aile içinde iki kez kurulmaz.
    if !seen.insert(postscript.clone()) {
        return Ok(false);
    }
    if coretext::is_registered(&target) && target.exists() {
        let same = std::fs::read(&target).map(|d| Sha256::digest(&d) == Sha256::digest(&data)).unwrap_or(false);
        if same {
            record(db, font_id, file, "typevault", "installed", Some(&target), None)?;
            return Ok(true);
        }
    }
    if let Some(existing) = coretext::locate(&postscript) {
        let same_name_elsewhere = coretext::available_postscript_names().contains(&postscript) && !is_inside(root, &existing);
        if same_name_elsewhere {
            // Başkasına ait font asla değiştirilmez veya sahiplenilmez.
            record(db, font_id, file, "external", "installed", Some(&existing), None)?;
            return Ok(false);
        }
    }
    record(db, font_id, file, "typevault", "installing", Some(&target), None)?;
    std::fs::create_dir_all(target.parent().expect("aile klasörü"))?;
    let tmp = target.with_extension("tvtmp");
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &target)?;
    let registered = coretext::register(std::slice::from_ref(&target)).and_then(|()| {
        if wait_until_registered(&target) {
            Ok(())
        } else {
            Err(AppError::Install(format!("{} macOS tarafından doğrulanamadı", file.filename)))
        }
    });
    if let Err(e) = registered {
        let _ = std::fs::remove_file(&target);
        return Err(e);
    }
    record(db, font_id, file, "typevault", "installed", Some(&target), None)?;
    Ok(true)
}

pub fn install_font(db: &Db, root: &Path, font_id: &str) -> Result<InstallReport> {
    license_allows(db, font_id)?;
    let (slug, files) = load_files(db, font_id)?;
    if files.is_empty() {
        return Err(AppError::Install("Önce font indirilmelidir".into()));
    }
    let mut report = InstallReport::default();
    let mut seen = HashSet::new();
    for file in &files {
        match install_one(db, root, &slug, font_id, file, &mut seen) {
            Ok(true) => report.installed += 1,
            Ok(false) => report.external += 1,
            Err(e) => {
                let message = e.to_string();
                record(db, font_id, file, "typevault", "failed", None, Some(&message))?;
                report.failed.push(format!("{}: {message}", file.filename));
            }
        }
    }
    let (status, error) = if !report.failed.is_empty() {
        ("install_failed", Some(report.failed.join("; ")))
    } else {
        ("installed", None)
    };
    db.conn()?.execute(
        "UPDATE fonts SET local_status=?2, local_error=?3 WHERE id=?1",
        params![font_id, status, error],
    )?;
    Ok(report)
}

/// Yalnız TYPEVAULT'un sahip olduğu ve kendi klasöründe duran dosyaları kaldırır.
pub fn uninstall_font(db: &Db, root: &Path, font_id: &str) -> Result<usize> {
    let rows: Vec<(i64, String)> = {
        let conn = db.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, installed_path FROM installations
             WHERE font_id=?1 AND owner='typevault' AND installed_path IS NOT NULL",
        )?;
        let rows = stmt.query_map([font_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let mut removed = 0;
    for (id, path) in &rows {
        let path = PathBuf::from(path);
        if !is_inside(root, &path) {
            return Err(AppError::Install(format!("{} TYPEVAULT'a ait değil", path.display())));
        }
        if path.exists() {
            coretext::unregister(std::slice::from_ref(&path))?;
            std::fs::remove_file(&path)?;
            if let Some(parent) = path.parent() {
                let _ = std::fs::remove_dir(parent);
            }
            removed += 1;
        }
        db.conn()?.execute("DELETE FROM installations WHERE id=?1", [id])?;
    }
    let conn = db.conn()?;
    let remaining: i64 = conn.query_row(
        "SELECT count(*) FROM installations WHERE font_id=?1 AND status='installed'",
        [font_id],
        |r| r.get(0),
    )?;
    if remaining > 0 && rows.is_empty() {
        return Err(AppError::Install("Bu font TYPEVAULT tarafından kurulmadı; kaldırılamaz".into()));
    }
    conn.execute("DELETE FROM installations WHERE font_id=?1 AND status<>'installed'", [font_id])?;
    conn.execute(
        "UPDATE fonts SET local_error=NULL, local_status = CASE
           WHEN ?2 > 0 THEN 'installed'
           WHEN EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=?1 AND ff.local_path IS NOT NULL) THEN 'downloaded'
           ELSE 'available' END WHERE id=?1",
        params![font_id, remaining],
    )?;
    Ok(removed)
}

/// Veritabanını sistemin gerçek durumuyla eşitler: yarıda kalan kurulumlar ve elle silinen dosyalar.
pub fn reconcile(db: &Db, root: &Path) -> Result<()> {
    let rows: Vec<(i64, String, String, Option<String>, String)> = {
        let conn = db.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, font_id, status, installed_path, owner FROM installations",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    let mut touched = HashSet::new();
    for (id, font_id, status, path, owner) in rows {
        let path = path.map(PathBuf::from);
        if owner == "external" {
            // Dışarıdan kurulan font silinmişse artık "kurulu" sayılmaz; dosyaya hiçbir zaman dokunulmaz.
            if !path.as_deref().is_some_and(Path::exists) {
                db.conn()?.execute("DELETE FROM installations WHERE id=?1", [id])?;
                touched.insert(font_id);
            }
            continue;
        }
        let live = path.as_deref().filter(|p| p.exists() && is_inside(root, p));
        match (status.as_str(), live) {
            // Başarısız kayıt, kullanıcı yeniden deneyene dek durumu taşır.
            ("failed", _) => {}
            ("installed", Some(p)) if coretext::is_registered(p) => {}
            ("installed", Some(p)) => {
                if coretext::register(std::slice::from_ref(&p.to_path_buf())).is_err() || !wait_until_registered(p) {
                    db.conn()?.execute(
                        "UPDATE installations SET status='failed', error='macOS kaydı kaybolmuş' WHERE id=?1",
                        [id],
                    )?;
                    touched.insert(font_id);
                }
            }
            ("installing", Some(p)) => {
                let _ = coretext::unregister(std::slice::from_ref(&p.to_path_buf()));
                let _ = std::fs::remove_file(p);
                db.conn()?.execute("DELETE FROM installations WHERE id=?1", [id])?;
                touched.insert(font_id);
            }
            (_, None) => {
                db.conn()?.execute("DELETE FROM installations WHERE id=?1", [id])?;
                touched.insert(font_id);
            }
            _ => {}
        }
    }
    let conn = db.conn()?;
    for font_id in touched {
        conn.execute(
            "UPDATE fonts SET local_status = CASE
               WHEN EXISTS (SELECT 1 FROM installations i WHERE i.font_id=?1 AND i.status='failed') THEN 'install_failed'
               WHEN EXISTS (SELECT 1 FROM installations i WHERE i.font_id=?1 AND i.status='installed') THEN 'installed'
               WHEN EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=?1 AND ff.local_path IS NOT NULL) THEN 'downloaded'
               ELSE 'available' END WHERE id=?1",
            [&font_id],
        )?;
    }
    // Elle silinen indirmeler: kayıt dosyayla eşitlenir, aile "indirildi" görünmez.
    let local: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, local_path FROM font_files WHERE local_path IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<std::result::Result<_, _>>()?
    };
    for (id, path) in local {
        if !Path::new(&path).exists() {
            conn.execute("UPDATE font_files SET local_path=NULL, sha256=NULL WHERE id=?1", [id])?;
        }
    }
    conn.execute(
        "UPDATE fonts SET local_status='available', local_error=NULL, downloaded_version=NULL, update_available=0
         WHERE local_status='downloaded'
           AND NOT EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=fonts.id AND ff.local_path IS NOT NULL)",
        [],
    )?;
    // Süreç kurulum ortasında durduysa dosyaların bir kısmı kurulu, aile durumu eski kalır.
    conn.execute(
        "UPDATE fonts SET local_status='install_failed', local_error='Kurulum yarıda kaldı; yeniden deneyin'
         WHERE local_status<>'installed'
           AND EXISTS (SELECT 1 FROM installations i WHERE i.font_id=fonts.id AND i.status='installed')
           AND EXISTS (SELECT 1 FROM font_files ff WHERE ff.font_id=fonts.id
             AND NOT EXISTS (SELECT 1 FROM installations i WHERE i.file_id=ff.id))",
        [],
    )?;
    Ok(())
}

pub fn scan_system(db: &Db, root: &Path) -> Result<usize> {
    let fonts = coretext::scan_system_fonts();
    let mut conn = db.conn()?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM system_fonts", [])?;
    {
        let mut ins = tx.prepare(
            "INSERT OR REPLACE INTO system_fonts (postscript_name, family, style, path, managed) VALUES (?1,?2,?3,?4,?5)",
        )?;
        for f in &fonts {
            let managed = f.path.as_deref().is_some_and(|p| is_inside(root, p));
            ins.execute(params![
                f.postscript_name,
                f.family,
                f.style,
                f.path.as_ref().map(|p| p.to_string_lossy().to_string()),
                managed
            ])?;
        }
    }
    tx.commit()?;
    Ok(fonts.len())
}
