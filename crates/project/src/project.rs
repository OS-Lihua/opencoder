//! Project CRUD and instance context.
//!
//! Mirrors `src/project/project.ts` and `src/project/instance.ts`.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::debug;

use opencoder_core::id::{Identifier, Prefix};
use opencoder_core::storage::Database;

/// Project metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Project {
    pub id: String,
    pub worktree: String,
    pub vcs: Option<serde_json::Value>,
    pub name: String,
    pub icon_url: Option<String>,
    pub icon_color: Option<String>,
    pub sandbox: Option<String>,
    pub commands: Option<serde_json::Value>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// Project service: CRUD + ensure logic.
pub struct ProjectService {
    db: Arc<Database>,
}

impl ProjectService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Ensure a project exists for the given directory. Creates one if needed.
    pub fn ensure(&self, directory: &Path) -> Result<Project> {
        let worktree = directory.to_string_lossy().to_string();

        // Try to find existing
        if let Ok(existing) = self.get_by_worktree(&worktree) {
            return Ok(existing);
        }

        // Create new project
        let id = Identifier::create(Prefix::Project).into_string();
        let now = Utc::now().timestamp_millis();
        let name = directory
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());

        // Detect VCS
        let vcs = detect_vcs(directory);

        let project = Project {
            id: id.clone(),
            worktree: worktree.clone(),
            vcs,
            name,
            icon_url: None,
            icon_color: None,
            sandbox: None,
            commands: None,
            time_created: now,
            time_updated: now,
        };

        self.db.use_conn(|conn| {
            conn.execute(
                "INSERT INTO project (id, worktree, vcs, name, icon_url, icon_color, sandbox, commands, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    &project.id,
                    &project.worktree,
                    project.vcs.as_ref().map(|v| v.to_string()),
                    &project.name,
                    &project.icon_url,
                    &project.icon_color,
                    &project.sandbox,
                    project.commands.as_ref().map(|v| v.to_string()),
                    project.time_created,
                    project.time_updated,
                ],
            )?;
            Ok(())
        })?;

        debug!(project_id = %id, worktree = %worktree, "project created");
        Ok(project)
    }

    /// Get a project by ID.
    pub fn get(&self, project_id: &str) -> Result<Project> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, worktree, vcs, name, icon_url, icon_color, sandbox, commands, time_created, time_updated FROM project WHERE id = ?1",
            )?;
            stmt.query_row(rusqlite::params![project_id], row_to_project)
                .context("project not found")
        })
    }

    /// Get a project by worktree path.
    pub fn get_by_worktree(&self, worktree: &str) -> Result<Project> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, worktree, vcs, name, icon_url, icon_color, sandbox, commands, time_created, time_updated FROM project WHERE worktree = ?1",
            )?;
            stmt.query_row(rusqlite::params![worktree], row_to_project)
                .context("project not found")
        })
    }

    /// List all projects.
    pub fn list(&self) -> Result<Vec<Project>> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, worktree, vcs, name, icon_url, icon_color, sandbox, commands, time_created, time_updated FROM project ORDER BY time_updated DESC",
            )?;
            let projects = stmt
                .query_map([], row_to_project)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(projects)
        })
    }

    /// Update project name.
    pub fn update_name(&self, project_id: &str, name: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.db.use_conn(|conn| {
            conn.execute(
                "UPDATE project SET name = ?1, time_updated = ?2 WHERE id = ?3",
                rusqlite::params![name, now, project_id],
            )?;
            Ok(())
        })
    }
}

fn row_to_project(row: &rusqlite::Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        worktree: row.get(1)?,
        vcs: row
            .get::<_, Option<String>>(2)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        name: row.get(3)?,
        icon_url: row.get(4)?,
        icon_color: row.get(5)?,
        sandbox: row.get(6)?,
        commands: row
            .get::<_, Option<String>>(7)?
            .and_then(|s| serde_json::from_str(&s).ok()),
        time_created: row.get(8)?,
        time_updated: row.get(9)?,
    })
}

/// Detect version control system for a directory.
fn detect_vcs(dir: &Path) -> Option<serde_json::Value> {
    if dir.join(".git").exists() {
        Some(serde_json::json!({"type": "git"}))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Arc<Database>, ProjectService) {
        let db = Database::open_memory().unwrap();
        let svc = ProjectService::new(db.clone());
        (db, svc)
    }

    #[test]
    fn ensure_creates_project() {
        let (_db, svc) = setup();
        let project = svc.ensure(Path::new("/tmp/my-project")).unwrap();
        assert_eq!(project.name, "my-project");
        assert_eq!(project.worktree, "/tmp/my-project");
    }

    #[test]
    fn ensure_idempotent() {
        let (_db, svc) = setup();
        let p1 = svc.ensure(Path::new("/tmp/test")).unwrap();
        let p2 = svc.ensure(Path::new("/tmp/test")).unwrap();
        assert_eq!(p1.id, p2.id);
    }

    #[test]
    fn get_by_id() {
        let (_db, svc) = setup();
        let project = svc.ensure(Path::new("/tmp/test")).unwrap();
        let fetched = svc.get(&project.id).unwrap();
        assert_eq!(fetched.worktree, "/tmp/test");
    }

    #[test]
    fn list_projects() {
        let (_db, svc) = setup();
        svc.ensure(Path::new("/tmp/a")).unwrap();
        svc.ensure(Path::new("/tmp/b")).unwrap();
        let projects = svc.list().unwrap();
        assert_eq!(projects.len(), 2);
    }

    #[test]
    fn update_name() {
        let (_db, svc) = setup();
        let project = svc.ensure(Path::new("/tmp/test")).unwrap();
        svc.update_name(&project.id, "new-name").unwrap();
        let fetched = svc.get(&project.id).unwrap();
        assert_eq!(fetched.name, "new-name");
    }
}
