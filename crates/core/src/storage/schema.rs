//! Database table row types for type-safe access.
//!
//! These mirror the Drizzle schema from the original OpenCode's session.sql.ts.

use serde::{Deserialize, Serialize};

/// A row from the `project` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub worktree: String,
    pub vcs: Option<String>,
    pub name: String,
    pub icon_url: Option<String>,
    pub icon_color: Option<String>,
    pub sandbox: Option<String>,
    pub commands: Option<String>,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the `session` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRow {
    pub id: String,
    pub project_id: String,
    pub workspace_id: Option<String>,
    pub parent_id: Option<String>,
    pub slug: String,
    pub directory: String,
    pub title: String,
    pub version: String,
    pub share_url: Option<String>,
    pub summary_additions: Option<i64>,
    pub summary_deletions: Option<i64>,
    pub summary_files: Option<i64>,
    pub summary_diffs: Option<String>, // JSON
    pub revert: Option<String>,        // JSON
    pub permission: Option<String>,    // JSON
    pub time_created: i64,
    pub time_updated: i64,
    pub time_compacting: Option<i64>,
    pub time_archived: Option<i64>,
}

/// A row from the `message` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageRow {
    pub id: String,
    pub session_id: String,
    pub time_created: i64,
    pub time_updated: i64,
    pub data: String, // JSON
}

/// A row from the `part` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartRow {
    pub id: String,
    pub message_id: String,
    pub session_id: String,
    pub time_created: i64,
    pub time_updated: i64,
    pub data: String, // JSON
}

/// A row from the `todo` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoRow {
    pub session_id: String,
    pub content: String,
    pub status: String,
    pub priority: String,
    pub position: i64,
    pub time_created: i64,
    pub time_updated: i64,
}

/// A row from the `permission` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRow {
    pub project_id: String,
    pub time_created: i64,
    pub time_updated: i64,
    pub data: String, // JSON
}

/// A row from the `workspace` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub time_created: i64,
    pub time_updated: i64,
}
