//! SQLite-opslag (WAL) met migratiesysteem op `PRAGMA user_version`.
//! Eén schrijfverbinding (achter een mutex) + losse leesverbindingen voor snapshot-reads.
// VX:W01.01

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use rusqlite::{Connection, OpenFlags, Transaction};

use crate::error::{CoreError, Result};

/// Eén migratie: versie + SQL. Versies oplopend, nooit wijzigen na release — alleen toevoegen.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

/// Alle migraties van de kern.
pub const CORE_MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "kern: event log, runs, checkpoints, effecten, resources, config",
        sql: include_str!("../migrations/0001_core.sql"),
    },
    Migration { version: 2, name: "sessies en berichten", sql: include_str!("../migrations/0002_sessions.sql") },
];

/// Database-handle. Goedkoop te delen via `Arc`.
#[derive(Debug)]
pub struct Db {
    path: Option<PathBuf>,
    writer: Mutex<Connection>,
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

impl Db {
    /// Open (of maak) de database op `path` en voer migraties uit.
    pub fn open(path: impl AsRef<Path>, migrations: &[Migration]) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(&path)?;
        configure(&conn)?;
        let db = Self { path: Some(path), writer: Mutex::new(conn) };
        db.migrate(migrations)?;
        Ok(db)
    }

    /// In-memory database (tests). Snapshot-reads lezen dan via de schrijfverbinding.
    pub fn open_in_memory(migrations: &[Migration]) -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let db = Self { path: None, writer: Mutex::new(conn) };
        db.migrate(migrations)?;
        Ok(db)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Huidige schemaversie.
    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.writer.lock().pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    fn migrate(&self, migrations: &[Migration]) -> Result<()> {
        let mut conn = self.writer.lock();
        let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let max_known = migrations.iter().map(|m| m.version).max().unwrap_or(0);
        if current > max_known {
            return Err(CoreError::Migration(format!(
                "database heeft schema {current}, deze build kent maximaal {max_known}"
            )));
        }
        let mut sorted: Vec<_> = migrations.iter().filter(|m| m.version > current).collect();
        sorted.sort_by_key(|m| m.version);
        for m in sorted {
            let tx = conn.transaction()?;
            tx.execute_batch(m.sql).map_err(|e| CoreError::Migration(format!("{} ({}): {e}", m.version, m.name)))?;
            tx.pragma_update(None, "user_version", m.version)?;
            tx.commit()?;
            tracing::info!(version = m.version, name = m.name, "migratie toegepast");
        }
        Ok(())
    }

    /// Voer `f` uit in één schrijftransactie (outbox: status + events samen, of niets).
    pub fn write<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        let mut conn = self.writer.lock();
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let out = f(&tx)?;
        tx.commit()?;
        Ok(out)
    }

    /// Snapshot-consistente read: alles in `f` ziet dezelfde toestand van de database.
    pub fn read<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        match &self.path {
            Some(p) => {
                let conn =
                    Connection::open_with_flags(p, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
                conn.busy_timeout(std::time::Duration::from_secs(5))?;
                conn.execute_batch("BEGIN DEFERRED")?;
                let out = f(&conn);
                // Read-only: rollback is altijd veilig.
                let _ = conn.execute_batch("ROLLBACK");
                out
            }
            None => f(&self.writer.lock()),
        }
    }

    /// WAL-checkpoint (onderhoud).
    pub fn checkpoint_wal(&self) -> Result<()> {
        self.writer.lock().execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_record_version() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("v.db");
        let db = Db::open(&p, CORE_MIGRATIONS)?;
        assert_eq!(db.schema_version()?, 2);
        drop(db);
        let db = Db::open(&p, CORE_MIGRATIONS)?;
        assert_eq!(db.schema_version()?, 2);
        Ok(())
    }

    #[test]
    fn newer_schema_is_refused() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let p = dir.path().join("v.db");
        let db = Db::open(&p, CORE_MIGRATIONS)?;
        db.write(|tx| Ok(tx.pragma_update(None, "user_version", 99)?))?;
        drop(db);
        assert!(matches!(Db::open(&p, CORE_MIGRATIONS), Err(CoreError::Migration(_))));
        Ok(())
    }

    #[test]
    fn failed_write_rolls_back() -> Result<()> {
        let db = Db::open_in_memory(CORE_MIGRATIONS)?;
        let r: Result<()> = db.write(|tx| {
            tx.execute("INSERT INTO config_versions(version, hash, ts, content) VALUES ('a','h','t','c')", [])?;
            Err(CoreError::Config("boem".into()))
        });
        assert!(r.is_err());
        let n: i64 = db.read(|c| Ok(c.query_row("SELECT COUNT(*) FROM config_versions", [], |r| r.get(0))?))?;
        assert_eq!(n, 0);
        Ok(())
    }
}
