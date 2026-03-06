//! SQLite storage layer with transaction support and post-commit effects.
//!
//! Mirrors `src/storage/db.ts` from the original OpenCode.
//! Uses rusqlite with WAL mode and aggressive performance pragmas.

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tracing::info;

pub mod migration;
pub mod schema;

/// Database wrapper with transaction support and effect queuing.
pub struct Database {
    conn: Mutex<Connection>,
    /// Queued side-effects to run after transaction commit.
    effects: Mutex<Vec<Box<dyn FnOnce() + Send>>>,
}

impl Database {
    /// Open or create a database at the given path.
    pub fn open(path: &Path) -> anyhow::Result<Arc<Self>> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        // Apply performance pragmas (mirrors original)
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -64000;
            PRAGMA foreign_keys = ON;
            PRAGMA wal_checkpoint(PASSIVE);
            ",
        )?;

        let db = Arc::new(Self {
            conn: Mutex::new(conn),
            effects: Mutex::new(Vec::new()),
        });

        // Run migrations
        migration::run(&db)?;

        info!("database opened: {}", path.display());
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> anyhow::Result<Arc<Self>> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        let db = Arc::new(Self {
            conn: Mutex::new(conn),
            effects: Mutex::new(Vec::new()),
        });

        migration::run(&db)?;
        Ok(db)
    }

    /// Execute a closure with the database connection.
    /// Any queued effects are flushed after the closure returns.
    pub fn use_conn<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let result = f(&conn)?;
        drop(conn);

        // Flush effects
        self.flush_effects();
        Ok(result)
    }

    /// Run a closure inside an explicit transaction.
    /// Effects are flushed after successful commit.
    pub fn transaction<T, F>(&self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Connection) -> anyhow::Result<T>,
    {
        let mut conn = self.conn.lock().map_err(|e| anyhow::anyhow!("lock: {e}"))?;
        let tx = conn.transaction()?;
        let result = f(&tx)?;
        tx.commit()?;
        drop(conn);

        self.flush_effects();
        Ok(result)
    }

    /// Queue a side-effect to run after the current transaction commits.
    /// This is the Rust equivalent of `Database.effect()` in the original.
    pub fn effect<F: FnOnce() + Send + 'static>(&self, f: F) {
        let mut effects = self.effects.lock().unwrap();
        effects.push(Box::new(f));
    }

    /// Flush and execute all queued effects.
    fn flush_effects(&self) {
        let effects: Vec<_> = {
            let mut lock = self.effects.lock().unwrap();
            std::mem::take(&mut *lock)
        };
        for f in effects {
            f();
        }
    }

    /// Close the database connection.
    pub fn close(self) {
        if let Ok(conn) = self.conn.into_inner() {
            let _ = conn.close();
        }
    }

    /// Direct connection access (use sparingly).
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

/// Error for record not found.
#[derive(Debug, thiserror::Error)]
#[error("record not found: {entity} {id}")]
pub struct NotFoundError {
    pub entity: String,
    pub id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn open_memory_db() {
        let db = Database::open_memory().unwrap();
        db.use_conn(|conn| {
            let count: i64 =
                conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))?;
            assert!(count >= 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn effects_run_after_transaction() {
        let db = Database::open_memory().unwrap();
        let counter = Arc::new(AtomicU32::new(0));
        let c = counter.clone();

        db.transaction(|_conn| {
            db.effect(move || {
                c.fetch_add(1, Ordering::Relaxed);
            });
            Ok(())
        })
        .unwrap();

        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
