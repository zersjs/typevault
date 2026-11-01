pub mod migrations;

use crate::error::Result;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::time::Duration;

pub type Pool = r2d2::Pool<SqliteConnectionManager>;
pub type Conn = r2d2::PooledConnection<SqliteConnectionManager>;

#[derive(Clone)]
pub struct Db {
    pool: Pool,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let manager = SqliteConnectionManager::file(path).with_init(|c| {
            c.execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=8000;",
            )
        });
        let pool = r2d2::Pool::builder()
            .max_size(8)
            .connection_timeout(Duration::from_secs(15))
            .build(manager)?;
        let mut conn = pool.get()?;
        migrate(&mut conn)?;
        Ok(Self { pool })
    }

    pub fn conn(&self) -> Result<Conn> {
        Ok(self.pool.get()?)
    }

    /// SQLite çağrısını async çalışma zamanını bloklamadan yürütür.
    pub async fn run<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Conn) -> Result<T> + Send + 'static,
    {
        let db = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = db.conn()?;
            f(&mut conn)
        })
        .await
        .map_err(|e| crate::error::AppError::Invalid(format!("Arka plan görevi durdu: {e}")))?
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn()?;
        get_setting(&conn, key)
    }

    pub fn setting_or(&self, key: &str, fallback: &str) -> String {
        self.setting(key).ok().flatten().unwrap_or_else(|| fallback.to_string())
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn()?;
        set_setting(&conn, key, value)
    }
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (index, sql) in migrations::MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
pub fn test_db() -> (tempfile::TempDir, Db) {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("t.sqlite")).unwrap();
    (dir, db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let (dir, db) = test_db();
        db.set_setting("a", "1").unwrap();
        drop(db);
        let db = Db::open(&dir.path().join("t.sqlite")).unwrap();
        assert_eq!(db.setting("a").unwrap().as_deref(), Some("1"));
        let count: i64 = db.conn().unwrap().query_row("SELECT count(*) FROM licenses", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 3);
    }
}
