//! Database migration system.
//!
//! Mirrors the Drizzle migration system from the original OpenCode.
//! Migrations are embedded at compile time and applied in order.

use rusqlite::params;
use tracing::info;

use super::Database;

/// Run all pending migrations.
pub fn run(db: &Database) -> anyhow::Result<()> {
    db.use_conn(|conn| {
        // Create migrations tracking table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _migrations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                applied_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
            );",
        )?;

        // Get already-applied migrations
        let mut stmt = conn.prepare("SELECT name FROM _migrations ORDER BY name")?;
        let applied: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        // Apply pending migrations
        for (name, sql) in MIGRATIONS {
            if !applied.contains(&name.to_string()) {
                info!("applying migration: {name}");
                conn.execute_batch(sql)?;
                conn.execute(
                    "INSERT INTO _migrations (name) VALUES (?1)",
                    params![name],
                )?;
            }
        }

        Ok(())
    })
}

/// Embedded migrations in order. Each is (name, SQL).
const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_initial", include_str!("migrations/0001_initial.sql")),
];
