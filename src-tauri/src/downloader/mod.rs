//! Kalıcı indirme kuyruğu: sınırlı eşzamanlılık, Range ile devam, yeniden deneme.
pub mod license;

use crate::archive;
use crate::database::Db;
use crate::error::{AppError, Result};
use crate::fonts::parse;
use crate::metadata::{google, sync};
use crate::security::{check_trusted_url, safe_join, sanitize_filename, MAX_FONT_BYTES};
use crate::state::AppState;
use futures_util::StreamExt;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const MAX_ATTEMPTS: i64 = 3;
const FILE_RETRIES: u32 = 3;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stop {
    Pause,
    Cancel,
}

struct Running {
    token: CancellationToken,
    stop: Arc<Mutex<Option<Stop>>>,
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub id: i64,
    pub font_id: String,
    pub status: String,
    pub received_bytes: i64,
    pub files_done: i64,
    pub files_total: i64,
    /// Geçerli dosyanın tamamlanan oranı (0–1).
    pub file_fraction: f64,
    pub speed_bps: f64,
    pub current_file: Option<String>,
    pub error: Option<String>,
}

pub struct DownloadManager {
    db: Db,
    http: reqwest::Client,
    app: AppHandle,
    wake: Notify,
    running: Mutex<HashMap<i64, Running>>,
    live: Mutex<HashMap<i64, (Progress, bool)>>,
    state: Mutex<Weak<AppState>>,
}

struct Job {
    id: i64,
    font_id: String,
    family: String,
    slug: String,
    install_after: bool,
    batch_id: Option<i64>,
    api_files: Option<String>,
    catalog_license: Option<String>,
    last_modified: Option<String>,
}

struct Target {
    remote_name: String,
    filename: String,
    url: String,
}

impl DownloadManager {
    pub fn new(db: Db, http: reqwest::Client, app: AppHandle) -> Arc<Self> {
        Arc::new(Self {
            db,
            http,
            app,
            wake: Notify::new(),
            running: Mutex::default(),
            live: Mutex::default(),
            state: Mutex::new(Weak::new()),
        })
    }

    pub fn attach(self: &Arc<Self>, state: &Arc<AppState>) {
        *self.state.lock().unwrap() = Arc::downgrade(state);
    }

    /// Önceki oturumda yarım kalan işleri kuyruğa döndürür ve dağıtıcıyı başlatır.
    pub fn start(self: &Arc<Self>) -> Result<()> {
        self.db.conn()?.execute(
            "UPDATE downloads SET status='queued' WHERE status='downloading'",
            [],
        )?;
        let manager = self.clone();
        tauri::async_runtime::spawn(async move { manager.dispatch().await });
        let ticker = self.clone();
        tauri::async_runtime::spawn(async move { ticker.tick().await });
        Ok(())
    }

    pub fn notify(&self) {
        self.wake.notify_one();
    }

    pub fn enqueue(&self, font_id: &str, install_after: bool, batch_id: Option<i64>) -> Result<()> {
        let conn = self.db.conn()?;
        let existing: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, status FROM downloads WHERE font_id=?1 AND status IN ('queued','downloading','paused')",
                [font_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match existing {
            Some((id, status)) => {
                // Kuyruktaki işe kurulum isteği ve toplu iş eklenir, ikinci iş açılmaz.
                conn.execute(
                    "UPDATE downloads SET install_after = max(install_after, ?2), batch_id = COALESCE(?3, batch_id),
                       status = CASE WHEN status='paused' THEN 'queued' ELSE status END WHERE id=?1",
                    params![id, install_after, batch_id],
                )?;
                if status == "paused" {
                    self.notify();
                }
            }
            None => {
                conn.execute(
                    "INSERT INTO downloads (font_id, batch_id, status, install_after) VALUES (?1,?2,'queued',?3)",
                    params![font_id, batch_id, install_after],
                )?;
                self.notify();
            }
        }
        self.publish_row(font_id);
        Ok(())
    }

    fn publish_row(&self, font_id: &str) {
        let _ = self.app.emit("library://font-changed", font_id.to_string());
    }

    fn stop(&self, id: i64, kind: Stop) -> bool {
        match self.running.lock().unwrap().get(&id) {
            Some(run) => {
                *run.stop.lock().unwrap() = Some(kind);
                run.token.cancel();
                true
            }
            None => false,
        }
    }

    pub fn pause(&self, id: i64) -> Result<()> {
        if !self.stop(id, Stop::Pause) {
            self.db.conn()?.execute(
                "UPDATE downloads SET status='paused', updated_at=datetime('now') WHERE id=?1 AND status='queued'",
                [id],
            )?;
        }
        Ok(())
    }

    pub fn resume(&self, id: i64) -> Result<()> {
        self.db.conn()?.execute(
            "UPDATE downloads SET status='queued', attempts=0, error=NULL, updated_at=datetime('now')
             WHERE id=?1 AND status IN ('paused','failed','cancelled')",
            [id],
        )?;
        self.notify();
        Ok(())
    }

    pub fn cancel(&self, id: i64) -> Result<()> {
        if !self.stop(id, Stop::Cancel) {
            let conn = self.db.conn()?;
            conn.execute(
                "UPDATE downloads SET status='cancelled', updated_at=datetime('now') WHERE id=?1 AND status IN ('queued','paused')",
                [id],
            )?;
            let batch: Option<(Option<i64>, String)> = conn
                .query_row("SELECT batch_id, font_id FROM downloads WHERE id=?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            if let Some((batch, font)) = batch {
                if let Some(batch) = batch {
                    mark_batch(&self.db, batch, &font, "skipped", Some("İptal edildi"))?;
                }
                let slug: String = conn.query_row("SELECT slug FROM fonts WHERE id=?1", [&font], |r| r.get(0))?;
                self.discard_unfinished(&font, &slug)?;
            }
        }
        Ok(())
    }

    /// İptal edilen işin izlerini siler; daha önce tamamlanmış indirmenin dosyalarına dokunmaz.
    fn discard_unfinished(&self, font_id: &str, slug: &str) -> Result<()> {
        let root = self.target_dir()?;
        let _ = std::fs::remove_dir_all(partial_dir(&root, slug));
        let recorded: bool = self.db.conn()?.query_row(
            "SELECT EXISTS (SELECT 1 FROM font_files WHERE font_id=?1 AND local_path IS NOT NULL)",
            [font_id],
            |r| r.get(0),
        )?;
        if !recorded {
            let _ = std::fs::remove_dir_all(safe_join(&root, slug)?);
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Vec<Progress> {
        self.live.lock().unwrap().values().map(|(p, _)| p.clone()).collect()
    }

    fn update(&self, id: i64, f: impl FnOnce(&mut Progress)) {
        let mut live = self.live.lock().unwrap();
        if let Some((progress, dirty)) = live.get_mut(&id) {
            f(progress);
            *dirty = true;
        }
    }

    /// İlerleme olaylarını yayınlar ve gerçek hızı son pencereden hesaplar.
    async fn tick(self: Arc<Self>) {
        let mut last_bytes: HashMap<i64, (i64, Instant)> = HashMap::new();
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        let mut persist_at = Instant::now();
        loop {
            interval.tick().await;
            let mut batch = Vec::new();
            {
                let mut live = self.live.lock().unwrap();
                for (id, (progress, dirty)) in live.iter_mut() {
                    let now = Instant::now();
                    let (prev, at) = last_bytes.entry(*id).or_insert((progress.received_bytes, now));
                    let dt = now.duration_since(*at).as_secs_f64();
                    if dt >= 0.5 {
                        let instant = (progress.received_bytes - *prev).max(0) as f64 / dt;
                        progress.speed_bps = progress.speed_bps * 0.6 + instant * 0.4;
                        *prev = progress.received_bytes;
                        *at = now;
                        *dirty = true;
                    }
                    if *dirty {
                        batch.push(progress.clone());
                        *dirty = false;
                    }
                }
                last_bytes.retain(|id, _| live.contains_key(id));
            }
            if !batch.is_empty() {
                let _ = self.app.emit("download://progress", &batch);
            }
            if persist_at.elapsed() > Duration::from_secs(2) && !batch.is_empty() {
                persist_at = Instant::now();
                let db = self.db.clone();
                let _ = tokio::task::spawn_blocking(move || -> Result<()> {
                    let conn = db.conn()?;
                    for p in batch {
                        conn.execute(
                            "UPDATE downloads SET received_bytes=?2, files_done=?3, files_total=?4 WHERE id=?1",
                            params![p.id, p.received_bytes, p.files_done, p.files_total],
                        )?;
                    }
                    Ok(())
                })
                .await;
            }
        }
    }

    async fn dispatch(self: Arc<Self>) {
        loop {
            let limit: usize = self.db.setting_or("download_concurrency", "4").parse().unwrap_or(4);
            let free = limit.clamp(1, 12).saturating_sub(self.running.lock().unwrap().len());
            if free > 0 {
                if let Ok(jobs) = self.claim(free).await {
                    for job in jobs {
                        self.launch(job);
                    }
                }
            }
            tokio::select! {
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    }

    async fn claim(&self, count: usize) -> Result<Vec<Job>> {
        self.db
            .run(move |conn| {
                // Okumadan yazmaya yükseltme eşzamanlı yazarla SQLITE_BUSY verir; kilit baştan alınır.
                let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
                let jobs: Vec<Job> = {
                    let mut stmt = tx.prepare(
                        "SELECT d.id, f.id, f.family, f.slug, d.install_after, d.batch_id, f.api_files, l.code, f.last_modified
                         FROM downloads d JOIN fonts f ON f.id = d.font_id LEFT JOIN licenses l ON l.id = f.license_id
                         WHERE d.status='queued' ORDER BY d.id LIMIT ?1",
                    )?;
                    let rows = stmt.query_map([count as i64], |r| {
                        Ok(Job {
                            id: r.get(0)?,
                            font_id: r.get(1)?,
                            family: r.get(2)?,
                            slug: r.get(3)?,
                            install_after: r.get(4)?,
                            batch_id: r.get(5)?,
                            api_files: r.get(6)?,
                            catalog_license: r.get(7)?,
                            last_modified: r.get(8)?,
                        })
                    })?;
                    rows.collect::<std::result::Result<_, _>>()?
                };
                for job in &jobs {
                    tx.execute(
                        "UPDATE downloads SET status='downloading', error=NULL, updated_at=datetime('now') WHERE id=?1",
                        [job.id],
                    )?;
                }
                tx.commit()?;
                Ok(jobs)
            })
            .await
    }

    fn launch(self: &Arc<Self>, job: Job) {
        let token = CancellationToken::new();
        let stop = Arc::new(Mutex::new(None));
        self.running
            .lock()
            .unwrap()
            .insert(job.id, Running { token: token.clone(), stop: stop.clone() });
        self.live.lock().unwrap().insert(
            job.id,
            (
                Progress { id: job.id, font_id: job.font_id.clone(), status: "downloading".into(), ..Default::default() },
                true,
            ),
        );
        self.publish_row(&job.font_id);
        let manager = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = manager.execute(&job, &token).await;
            let stop_kind = *stop.lock().unwrap();
            manager.finish(job, result, stop_kind).await;
        });
    }

    async fn finish(self: Arc<Self>, job: Job, result: Result<()>, stop: Option<Stop>) {
        let (status, error): (&str, Option<String>) = match (&result, stop) {
            (_, Some(Stop::Pause)) => ("paused", None),
            (_, Some(Stop::Cancel)) => ("cancelled", None),
            (Ok(()), None) => ("completed", None),
            (Err(e), None) => {
                let attempts: i64 = self
                    .db
                    .conn()
                    .and_then(|c| {
                        Ok(c.query_row(
                            "UPDATE downloads SET attempts=attempts+1 WHERE id=?1 RETURNING attempts",
                            [job.id],
                            |r| r.get(0),
                        )?)
                    })
                    .unwrap_or(MAX_ATTEMPTS);
                if e.is_transient() && attempts < MAX_ATTEMPTS {
                    tokio::time::sleep(Duration::from_secs(2u64.pow(attempts as u32))).await;
                    ("queued", None)
                } else {
                    ("failed", Some(e.to_string()))
                }
            }
        };
        if stop == Some(Stop::Cancel) {
            let _ = self.discard_unfinished(&job.font_id, &job.slug);
        }
        let snapshot = self.live.lock().unwrap().remove(&job.id).map(|(p, _)| p);
        let (received, done, total) =
            snapshot.as_ref().map(|p| (p.received_bytes, p.files_done, p.files_total)).unwrap_or((0, 0, 0));
        if let Ok(conn) = self.db.conn() {
            let _ = conn.execute(
                "UPDATE downloads SET status=?2, error=?3, received_bytes=?4, files_done=?5, files_total=?6, updated_at=datetime('now') WHERE id=?1",
                params![job.id, status, error, received, done, total],
            );
        }
        self.running.lock().unwrap().remove(&job.id);
        let _ = self.app.emit(
            "download://progress",
            vec![Progress {
                id: job.id,
                font_id: job.font_id.clone(),
                status: status.to_string(),
                received_bytes: received,
                files_done: done,
                files_total: total,
                error: error.clone(),
                ..Default::default()
            }],
        );
        match (status, job.batch_id) {
            ("completed", batch) if job.install_after => {
                if let Some(state) = self.state.lock().unwrap().upgrade() {
                    state.installer.enqueue(job.font_id.clone(), batch);
                }
            }
            ("completed", Some(batch)) => {
                let _ = mark_batch(&self.db, batch, &job.font_id, "done", None);
            }
            ("failed", Some(batch)) => {
                let _ = mark_batch(&self.db, batch, &job.font_id, "failed", error.as_deref());
            }
            ("cancelled", Some(batch)) => {
                let _ = mark_batch(&self.db, batch, &job.font_id, "skipped", Some("İptal edildi"));
            }
            _ => {}
        }
        let _ = self.app.emit("library://changed", ());
        self.notify();
    }

    fn target_dir(&self) -> Result<PathBuf> {
        let state = self.state.lock().unwrap().upgrade();
        state.map(|s| s.font_dir()).ok_or_else(|| AppError::Invalid("Uygulama kapanıyor".into()))
    }

    async fn execute(&self, job: &Job, token: &CancellationToken) -> Result<()> {
        let root = self.target_dir()?;
        let manifest = google::fetch_manifest(&self.http, &job.family).await;
        let pref = self.db.setting_or("font_format", "static");
        let (targets, license_code, license_file) = match manifest {
            Ok(m) => {
                let detected = license::detect(&m.files);
                let embedded = detected.map(|d| d.0);
                let catalog = match &job.catalog_license {
                    Some(c) => Some(c.clone()),
                    None => self.fetch_license(job).await,
                };
                let code = license::reconcile(catalog.as_deref(), embedded)?;
                let file = detected.map(|(_, f)| (f.filename.clone(), f.contents.clone()));
                (select_targets(&m.file_refs, &pref)?, code, file)
            }
            Err(e) => match api_targets(job)? {
                // Manifest alınamazsa Developer API adresleri kullanılabilir.
                Some(targets) if !targets.is_empty() => {
                    let catalog = job.catalog_license.clone();
                    (targets, license::reconcile(catalog.as_deref(), None)?, None)
                }
                _ => return Err(e),
            },
        };
        if targets.is_empty() {
            return Err(AppError::NotFound("indirilebilir TTF/OTF dosyası yok".into()));
        }
        self.update(job.id, |p| p.files_total = targets.len() as i64);

        let family_dir = safe_join(&root, &job.slug)?;
        let part_dir = partial_dir(&root, &job.slug);
        tokio::fs::create_dir_all(&family_dir).await?;
        tokio::fs::create_dir_all(&part_dir).await?;

        let mut turkish: Option<crate::fonts::turkish::Support> = None;
        let mut completed = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            if token.is_cancelled() {
                return Ok(());
            }
            self.update(job.id, |p| {
                p.files_done = index as i64;
                p.file_fraction = 0.0;
                p.current_file = Some(target.filename.clone());
            });
            let finals = self.fetch_file(job, target, &family_dir, &part_dir, token).await?;
            if token.is_cancelled() {
                return Ok(());
            }
            for item in finals {
                turkish = Some(weakest(turkish.take(), item.info.turkish.clone()));
                completed.push(item);
            }
        }
        self.update(job.id, |p| {
            p.files_done = targets.len() as i64;
            p.file_fraction = 0.0;
        });

        if let Some((name, contents)) = &license_file {
            tokio::fs::write(family_dir.join(sanitize_filename(name)?), contents).await?;
        }
        let _ = tokio::fs::remove_dir_all(&part_dir).await;
        let font_id = job.font_id.clone();
        let version = job.last_modified.clone();
        self.db
            .run(move |conn| {
                let tx = conn.transaction()?;
                let license: Option<i64> =
                    tx.query_row("SELECT id FROM licenses WHERE code=?1", [&license_code], |r| r.get(0)).optional()?;
                let license = license.ok_or_else(|| AppError::License(format!("bilinmeyen tür: {license_code}")))?;
                let keep: Vec<String> = completed.iter().map(|c| c.filename.clone()).collect();
                {
                    let mut existing = tx.prepare(
                        "SELECT ff.id, ff.filename, ff.local_path, i.id FROM font_files ff
                         LEFT JOIN installations i ON i.file_id = ff.id WHERE ff.font_id=?1",
                    )?;
                    let rows: Vec<(i64, String, Option<String>, Option<i64>)> = existing
                        .query_map([&font_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                        .collect::<std::result::Result<_, _>>()?;
                    // Kurulu olmayan eski dosyalar yeni tercihe göre temizlenir.
                    for (id, filename, path, installed) in rows {
                        if !keep.contains(&filename) && installed.is_none() {
                            if let Some(path) = path {
                                let _ = std::fs::remove_file(path);
                            }
                            tx.execute("DELETE FROM font_files WHERE id=?1", [id])?;
                        }
                    }
                }
                for c in &completed {
                    tx.execute(
                        "INSERT INTO font_files (font_id, filename, url, format, is_variable, local_path, sha256,
                           size_bytes, postscript_name, weight, italic)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                         ON CONFLICT(font_id, filename) DO UPDATE SET url=excluded.url, format=excluded.format,
                           is_variable=excluded.is_variable, local_path=excluded.local_path, sha256=excluded.sha256,
                           size_bytes=excluded.size_bytes, postscript_name=excluded.postscript_name,
                           weight=excluded.weight, italic=excluded.italic",
                        params![
                            font_id,
                            c.filename,
                            c.url,
                            c.info.format,
                            c.info.is_variable,
                            c.path.to_string_lossy(),
                            c.sha256,
                            c.size as i64,
                            c.info.postscript_name,
                            c.info.weight as i64,
                            c.info.italic,
                        ],
                    )?;
                }
                tx.execute(
                    "UPDATE fonts SET license_id=?2, downloaded_version=?3, update_available=0, local_error=NULL,
                       local_status = CASE WHEN local_status='available' THEN 'downloaded' ELSE local_status END WHERE id=?1",
                    params![font_id, license, version],
                )?;
                if let Some(support) = turkish {
                    tx.execute(
                        "UPDATE fonts SET turkish_status=?2, turkish_missing=?3, turkish_source='cmap' WHERE id=?1",
                        params![font_id, support.status, support.missing],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
    }

    async fn fetch_license(&self, job: &Job) -> Option<String> {
        let detail = google::fetch_detail(&self.http, &job.family).await.ok()?;
        let code = detail.license.clone();
        let (id, version) = (job.font_id.clone(), job.last_modified.clone());
        let _ = self
            .db
            .run(move |conn| sync::apply_detail(conn, &id, version.as_deref().or(Some("")), &detail))
            .await;
        code
    }

    /// Dosyayı indirir, doğrular ve yerine taşır. Geçerli yerel kopya varsa ağa çıkmaz.
    async fn fetch_file(
        &self,
        job: &Job,
        target: &Target,
        family_dir: &Path,
        part_dir: &Path,
        token: &CancellationToken,
    ) -> Result<Vec<Stored>> {
        let dest = family_dir.join(&target.filename);
        let marker = part_dir.join(format!("{}.done", target.filename));
        if let Ok(data) = tokio::fs::read(&dest).await {
            let known: Option<(String, String)> = self
                .db
                .conn()?
                .query_row(
                    "SELECT sha256, url FROM font_files WHERE font_id=?1 AND filename=?2",
                    params![job.font_id, target.filename],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let sha = hex::encode(Sha256::digest(&data));
            // Duraklatılan işte biten dosyalar henüz veritabanında değildir; işaret dosyası kanıttır.
            let marked = tokio::fs::read_to_string(&marker).await.is_ok_and(|m| m == format!("{sha} {}", target.url));
            if marked || known.is_some_and(|(hash, url)| hash == sha && url == target.url) {
                if let Ok(info) = parse::inspect(&data) {
                    self.update(job.id, |p| p.received_bytes += data.len() as i64);
                    return Ok(vec![Stored {
                        filename: target.filename.clone(),
                        url: target.url.clone(),
                        path: dest,
                        sha256: sha,
                        size: data.len() as u64,
                        info,
                    }]);
                }
            }
        }
        let part = part_dir.join(format!("{}.part", target.filename));
        let base = self.live.lock().unwrap().get(&job.id).map(|(p, _)| p.received_bytes).unwrap_or(0);
        let mut attempt = 0;
        let mut corrupt_retry = false;
        loop {
            attempt += 1;
            match self.stream_to_part(job, target, &part, base, token).await {
                Ok(()) if token.is_cancelled() => return Ok(Vec::new()),
                Ok(()) => {
                    let data = tokio::fs::read(&part).await?;
                    match self.store(target, data, family_dir).await {
                        Ok(stored) => {
                            let _ = tokio::fs::remove_file(&part).await;
                            if let [one] = stored.as_slice() {
                                tokio::fs::write(&marker, format!("{} {}", one.sha256, target.url)).await?;
                            }
                            return Ok(stored);
                        }
                        // Bozuk kısmi dosya sıfırdan bir kez daha denenir.
                        Err(AppError::Font(_)) if !corrupt_retry => {
                            corrupt_retry = true;
                            let _ = tokio::fs::remove_file(&part).await;
                        }
                        Err(e) => {
                            let _ = tokio::fs::remove_file(&part).await;
                            return Err(e);
                        }
                    }
                }
                Err(e) if e.is_transient() && attempt < FILE_RETRIES => {
                    tokio::select! {
                        _ = token.cancelled() => return Ok(Vec::new()),
                        _ = tokio::time::sleep(Duration::from_millis(600 * 2u64.pow(attempt))) => {}
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn store(&self, target: &Target, data: Vec<u8>, family_dir: &Path) -> Result<Vec<Stored>> {
        let dir = family_dir.to_path_buf();
        let target_name = target.filename.clone();
        let url = target.url.clone();
        tokio::task::spawn_blocking(move || -> Result<Vec<Stored>> {
            let items = if archive::is_zip(&data) {
                archive::extract_fonts(&data)?.into_iter().map(|e| (e.filename, e.data)).collect()
            } else {
                vec![(target_name, data)]
            };
            let mut out = Vec::new();
            for (filename, bytes) in items {
                let info = parse::inspect(&bytes)?;
                let path = dir.join(&filename);
                let tmp = dir.join(format!("{filename}.tmp"));
                std::fs::write(&tmp, &bytes)?;
                std::fs::rename(&tmp, &path)?;
                out.push(Stored {
                    filename,
                    url: url.clone(),
                    path,
                    sha256: hex::encode(Sha256::digest(&bytes)),
                    size: bytes.len() as u64,
                    info,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| AppError::Invalid(e.to_string()))?
    }

    async fn stream_to_part(&self, job: &Job, target: &Target, part: &Path, base: i64, token: &CancellationToken) -> Result<()> {
        let url = check_trusted_url(&target.url)?;
        let offset = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        // Range sıkıştırılmış temsile uygulanır; açık "identity" olmadan devam eden dosya kesik kalır.
        let mut request = self.http.get(url).header(reqwest::header::ACCEPT_ENCODING, "identity");
        if offset > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let response = tokio::select! {
            _ = token.cancelled() => return Ok(()),
            r = request.send() => r?,
        };
        let status = response.status();
        if status.as_u16() == 416 {
            // Sunucu aralığı reddettiyse kısmi dosya zaten tam olabilir ya da geçersizdir; doğrulama karar verir.
            return Ok(());
        }
        if !status.is_success() {
            return Err(AppError::Status(status.as_u16()));
        }
        let resumed = status.as_u16() == 206;
        let start = if resumed { offset } else { 0 };
        let length = total_length(&response, start);
        if length.is_some_and(|l| l > MAX_FONT_BYTES) {
            return Err(AppError::Font("dosya boyut sınırını aşıyor".into()));
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(part)
            .await?;
        let mut written = start;
        let mut stream = response.bytes_stream();
        loop {
            tokio::select! {
                _ = token.cancelled() => {
                    file.flush().await?;
                    return Ok(());
                }
                chunk = stream.next() => match chunk {
                    Some(chunk) => {
                        let chunk = chunk?;
                        file.write_all(&chunk).await?;
                        written += chunk.len() as u64;
                        if written > MAX_FONT_BYTES {
                            return Err(AppError::Font("dosya boyut sınırını aşıyor".into()));
                        }
                        let fraction = length.map(|l| written as f64 / l.max(1) as f64).unwrap_or(0.0);
                        self.update(job.id, |p| {
                            p.received_bytes = base + written as i64;
                            p.file_fraction = fraction;
                        });
                    }
                    None => break,
                }
            }
        }
        file.flush().await?;
        if length.is_some_and(|l| written < l) {
            return Err(AppError::Incomplete);
        }
        Ok(())
    }
}

/// Beklenen tam boyut; 206 yanıtında `Content-Range` sonundaki toplamdan okunur.
fn total_length(response: &reqwest::Response, start: u64) -> Option<u64> {
    let range_total = response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(content_range_total);
    range_total.or_else(|| response.content_length().map(|l| l + start))
}

fn content_range_total(value: &str) -> Option<u64> {
    value.rsplit('/').next()?.parse().ok()
}

struct Stored {
    filename: String,
    url: String,
    path: PathBuf,
    sha256: String,
    size: u64,
    info: parse::FontInfo,
}

fn partial_dir(root: &Path, slug: &str) -> PathBuf {
    root.join(".partial").join(slug)
}

/// Aile tam sayılabilmesi için tüm dosyalar gerekli glifleri taşımalıdır.
fn weakest(
    current: Option<crate::fonts::turkish::Support>,
    next: crate::fonts::turkish::Support,
) -> crate::fonts::turkish::Support {
    match current {
        Some(c) if c.missing.chars().count() >= next.missing.chars().count() => c,
        _ => next,
    }
}

/// Dosya seçimi: statik ve değişken kesimler birlikte kurulursa adlar çakışır, biri seçilir.
fn select_targets(refs: &[google::FileRef], pref: &str) -> Result<Vec<Target>> {
    let mut variable = Vec::new();
    let mut fixed = Vec::new();
    for r in refs {
        let lower = r.filename.to_lowercase();
        if lower.ends_with(".woff") || lower.ends_with(".woff2") {
            continue;
        }
        if !(lower.ends_with(".ttf") || lower.ends_with(".otf") || lower.ends_with(".zip")) {
            continue;
        }
        let target = Target {
            filename: sanitize_filename(&r.filename)?,
            remote_name: r.filename.clone(),
            url: r.url.clone(),
        };
        // Bazı aileler `Ad[wght].ttf`, bazıları `Ad-VariableFont_wght.ttf` biçimini kullanır.
        if r.filename.contains('[') || r.filename.contains("VariableFont") {
            variable.push(target);
        } else {
            fixed.push(target);
        }
    }
    let (first, second) = if pref == "variable" { (variable, fixed) } else { (fixed, variable) };
    let mut chosen = if first.is_empty() { second } else { first };
    chosen.sort_by(|a, b| a.remote_name.cmp(&b.remote_name));
    chosen.dedup_by(|a, b| a.filename == b.filename);
    Ok(chosen)
}

fn api_targets(job: &Job) -> Result<Option<Vec<Target>>> {
    let Some(raw) = &job.api_files else { return Ok(None) };
    let files: std::collections::BTreeMap<String, String> = serde_json::from_str(raw)?;
    let targets = files
        .into_iter()
        .filter(|(_, url)| url.ends_with(".ttf") || url.ends_with(".otf"))
        .map(|(variant, url)| {
            let ext = if url.ends_with(".otf") { "otf" } else { "ttf" };
            Ok(Target { filename: sanitize_filename(&format!("{}-{variant}.{ext}", job.slug))?, remote_name: variant, url })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(targets))
}

pub fn create_batch(db: &Db, label: &str, fonts: &[String], install_after: bool) -> Result<i64> {
    let mut conn = db.conn()?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO batches (label, total, install_after) VALUES (?1,?2,?3)",
        params![label, fonts.len() as i64, install_after],
    )?;
    let id = tx.last_insert_rowid();
    {
        let mut ins = tx.prepare("INSERT OR IGNORE INTO batch_fonts (batch_id, font_id) VALUES (?1,?2)")?;
        for font in fonts {
            ins.execute(params![id, font])?;
        }
    }
    tx.commit()?;
    Ok(id)
}

pub fn mark_batch(db: &Db, batch: i64, font: &str, state: &str, message: Option<&str>) -> Result<()> {
    db.conn()?.execute(
        "UPDATE batch_fonts SET state=?3, message=?4 WHERE batch_id=?1 AND font_id=?2",
        params![batch, font, state, message],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(name: &str) -> google::FileRef {
        google::FileRef { filename: name.into(), url: format!("https://fonts.gstatic.com/{name}") }
    }

    #[test]
    fn prefers_static_unless_asked() {
        let refs = [r("Roboto[wdth,wght].ttf"), r("static/Roboto-Thin.ttf"), r("static/Roboto-Bold.ttf"), r("a.woff2")];
        let picked = select_targets(&refs, "static").unwrap();
        assert_eq!(picked.iter().map(|t| t.filename.as_str()).collect::<Vec<_>>(), ["Roboto-Bold.ttf", "Roboto-Thin.ttf"]);
        let picked = select_targets(&refs, "variable").unwrap();
        assert_eq!(picked.len(), 1);
        assert!(picked[0].filename.contains('['));
    }

    #[test]
    fn recognizes_variable_files_without_brackets() {
        let refs = [r("NotoSerifJP-VariableFont_wght.ttf"), r("static/NotoSerifJP-Bold.ttf")];
        let picked = select_targets(&refs, "static").unwrap();
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].filename, "NotoSerifJP-Bold.ttf");
    }

    #[test]
    fn reads_total_from_content_range() {
        assert_eq!(content_range_total("bytes 7798784-8031691/8031692"), Some(8031692));
        assert_eq!(content_range_total("bytes 0-1/*"), None);
    }

    #[test]
    fn falls_back_when_only_one_kind_exists() {
        assert_eq!(select_targets(&[r("A[wght].ttf")], "static").unwrap().len(), 1);
        assert_eq!(select_targets(&[r("A-Regular.ttf")], "variable").unwrap().len(), 1);
    }
}
