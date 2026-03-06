//! Session CRUD operations.
//!
//! Mirrors `src/session/session.ts` from the original OpenCode.
//! Sessions are stored in SQLite with messages and parts.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::debug;

use opencoder_core::bus::{Bus, Event, SessionEvent};
use opencoder_core::id::{Identifier, Prefix};
use opencoder_core::storage::Database;

use crate::message::{Message, MessageWithParts, Part, PartWithId};

/// Session metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub workspace_id: Option<String>,
    pub parent_id: Option<String>,
    pub slug: String,
    pub directory: String,
    pub title: String,
    pub version: String,
    #[serde(default)]
    pub share_url: Option<String>,
    #[serde(default)]
    pub summary_additions: Option<i64>,
    #[serde(default)]
    pub summary_deletions: Option<i64>,
    #[serde(default)]
    pub summary_files: Option<i64>,
    #[serde(default)]
    pub summary_diffs: Option<serde_json::Value>,
    #[serde(default)]
    pub revert: Option<serde_json::Value>,
    #[serde(default)]
    pub permission: Option<serde_json::Value>,
    pub time_created: i64,
    pub time_updated: i64,
    #[serde(default)]
    pub time_compacting: Option<i64>,
    #[serde(default)]
    pub time_archived: Option<i64>,
}

/// Session service: CRUD operations on sessions, messages, and parts.
pub struct SessionService {
    db: Arc<Database>,
    bus: Bus,
}

impl SessionService {
    pub fn new(db: Arc<Database>, bus: Bus) -> Self {
        Self { db, bus }
    }

    /// Create a new session.
    pub fn create(
        &self,
        project_id: &str,
        directory: &str,
        workspace_id: Option<&str>,
    ) -> Result<Session> {
        let id = Identifier::descending(Prefix::Session).into_string();
        let now = Utc::now().timestamp_millis();
        let slug = generate_slug();
        let version = "2".to_string();

        let session = Session {
            id: id.clone(),
            project_id: project_id.to_string(),
            workspace_id: workspace_id.map(String::from),
            parent_id: None,
            slug: slug.clone(),
            directory: directory.to_string(),
            title: "New Session".to_string(),
            version,
            share_url: None,
            summary_additions: None,
            summary_deletions: None,
            summary_files: None,
            summary_diffs: None,
            revert: None,
            permission: None,
            time_created: now,
            time_updated: now,
            time_compacting: None,
            time_archived: None,
        };

        let bus = self.bus.clone();
        let title = session.title.clone();
        let sid = session.id.clone();

        self.db.transaction(|conn| {
            conn.execute(
                "INSERT INTO session (id, project_id, workspace_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    &session.id,
                    &session.project_id,
                    &session.workspace_id,
                    &session.parent_id,
                    &session.slug,
                    &session.directory,
                    &session.title,
                    &session.version,
                    session.time_created,
                    session.time_updated,
                ],
            )?;

            self.db.effect(move || {
                bus.publish(Event::SessionCreated(SessionEvent {
                    id: sid.parse().unwrap_or_else(|_| Identifier::create(Prefix::Session)),
                    title,
                }));
            });

            Ok(())
        })?;

        debug!(session_id = %id, "session created");
        Ok(session)
    }

    /// Get a session by ID.
    pub fn get(&self, session_id: &str) -> Result<Session> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, project_id, workspace_id, parent_id, slug, directory, title, version, share_url, summary_additions, summary_deletions, summary_files, summary_diffs, revert, permission, time_created, time_updated, time_compacting, time_archived FROM session WHERE id = ?1",
            )?;
            let session = stmt.query_row(rusqlite::params![session_id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    workspace_id: row.get(2)?,
                    parent_id: row.get(3)?,
                    slug: row.get(4)?,
                    directory: row.get(5)?,
                    title: row.get(6)?,
                    version: row.get(7)?,
                    share_url: row.get(8)?,
                    summary_additions: row.get(9)?,
                    summary_deletions: row.get(10)?,
                    summary_files: row.get(11)?,
                    summary_diffs: row.get::<_, Option<String>>(12)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    revert: row.get::<_, Option<String>>(13)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    permission: row.get::<_, Option<String>>(14)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    time_created: row.get(15)?,
                    time_updated: row.get(16)?,
                    time_compacting: row.get(17)?,
                    time_archived: row.get(18)?,
                })
            }).context("session not found")?;
            Ok(session)
        })
    }

    /// List sessions for a project, newest first.
    pub fn list(&self, project_id: &str) -> Result<Vec<Session>> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, project_id, workspace_id, parent_id, slug, directory, title, version, share_url, summary_additions, summary_deletions, summary_files, summary_diffs, revert, permission, time_created, time_updated, time_compacting, time_archived FROM session WHERE project_id = ?1 AND time_archived IS NULL ORDER BY id ASC",
            )?;
            let sessions = stmt.query_map(rusqlite::params![project_id], |row| {
                Ok(Session {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    workspace_id: row.get(2)?,
                    parent_id: row.get(3)?,
                    slug: row.get(4)?,
                    directory: row.get(5)?,
                    title: row.get(6)?,
                    version: row.get(7)?,
                    share_url: row.get(8)?,
                    summary_additions: row.get(9)?,
                    summary_deletions: row.get(10)?,
                    summary_files: row.get(11)?,
                    summary_diffs: row.get::<_, Option<String>>(12)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    revert: row.get::<_, Option<String>>(13)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    permission: row.get::<_, Option<String>>(14)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    time_created: row.get(15)?,
                    time_updated: row.get(16)?,
                    time_compacting: row.get(17)?,
                    time_archived: row.get(18)?,
                })
            })?.collect::<Result<Vec<_>, _>>()?;
            Ok(sessions)
        })
    }

    /// Delete a session and all its messages/parts (CASCADE).
    pub fn remove(&self, session_id: &str) -> Result<()> {
        let sid = session_id.to_string();
        let bus = self.bus.clone();

        self.db.transaction(|conn| {
            conn.execute("DELETE FROM session WHERE id = ?1", rusqlite::params![&sid])?;

            let sid2 = sid.clone();
            self.db.effect(move || {
                bus.publish(Event::SessionDeleted {
                    id: sid2.parse().unwrap_or_else(|_| Identifier::create(Prefix::Session)),
                });
            });
            Ok(())
        })?;

        debug!(session_id, "session removed");
        Ok(())
    }

    /// Update session title.
    pub fn set_title(&self, session_id: &str, title: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.db.use_conn(|conn| {
            conn.execute(
                "UPDATE session SET title = ?1, time_updated = ?2 WHERE id = ?3",
                rusqlite::params![title, now, session_id],
            )?;
            Ok(())
        })?;
        self.publish_session_updated(session_id, title);
        Ok(())
    }

    /// Archive a session.
    pub fn archive(&self, session_id: &str) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.db.use_conn(|conn| {
            conn.execute(
                "UPDATE session SET time_archived = ?1, time_updated = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            )?;
            Ok(())
        })
    }

    /// Update the share URL for a session.
    pub fn update_share_url(&self, session_id: &str, share_url: Option<&str>) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        self.db.use_conn(|conn| {
            conn.execute(
                "UPDATE session SET share_url = ?1, time_updated = ?2 WHERE id = ?3",
                rusqlite::params![share_url, now, session_id],
            )?;
            Ok(())
        })
    }

    /// Add a message to a session.
    pub fn add_message(&self, session_id: &str, message: &Message) -> Result<String> {
        let id = Identifier::ascending(Prefix::Message).into_string();
        let now = Utc::now().timestamp_millis();
        let data = serde_json::to_string(message)?;

        self.db.use_conn(|conn| {
            conn.execute(
                "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![&id, session_id, now, now, &data],
            )?;
            Ok(())
        })?;

        debug!(message_id = %id, session_id, "message added");
        Ok(id)
    }

    /// Add a part to a message.
    pub fn add_part(&self, session_id: &str, message_id: &str, part: &Part) -> Result<String> {
        let id = Identifier::ascending(Prefix::Part).into_string();
        let now = Utc::now().timestamp_millis();
        let data = serde_json::to_string(part)?;

        let bus = self.bus.clone();
        let sid = session_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Session));
        let mid = message_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Message));
        let pid = id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Part));

        self.db.use_conn(|conn| {
            conn.execute(
                "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![&id, message_id, session_id, now, now, &data],
            )?;

            self.db.effect(move || {
                bus.publish(Event::PartUpdated {
                    session_id: sid,
                    message_id: mid,
                    part_id: pid,
                });
            });

            Ok(())
        })?;

        Ok(id)
    }

    /// Update an existing part.
    pub fn update_part(&self, part_id: &str, part: &Part) -> Result<()> {
        let now = Utc::now().timestamp_millis();
        let data = serde_json::to_string(part)?;

        self.db.use_conn(|conn| {
            conn.execute(
                "UPDATE part SET data = ?1, time_updated = ?2 WHERE id = ?3",
                rusqlite::params![&data, now, part_id],
            )?;
            Ok(())
        })
    }

    /// Publish a delta update for streaming part content.
    pub fn publish_part_delta(
        &self,
        session_id: &str,
        message_id: &str,
        part_id: &str,
        field: &str,
        delta: &str,
    ) {
        self.bus.publish(Event::PartDelta {
            session_id: session_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Session)),
            message_id: message_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Message)),
            part_id: part_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Part)),
            field: field.to_string(),
            delta: delta.to_string(),
        });
    }

    /// Get all messages with their parts for a session.
    pub fn messages(&self, session_id: &str) -> Result<Vec<MessageWithParts>> {
        self.db.use_conn(|conn| {
            // Fetch messages
            let mut msg_stmt = conn.prepare(
                "SELECT id, session_id, time_created, time_updated, data FROM message WHERE session_id = ?1 ORDER BY id ASC",
            )?;
            let msg_rows: Vec<(String, String, i64, i64, String)> = msg_stmt
                .query_map(rusqlite::params![session_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            // Fetch all parts for this session
            let mut part_stmt = conn.prepare(
                "SELECT id, message_id, session_id, time_created, time_updated, data FROM part WHERE session_id = ?1 ORDER BY id ASC",
            )?;
            let part_rows: Vec<(String, String, String, i64, i64, String)> = part_stmt
                .query_map(rusqlite::params![session_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;

            // Group parts by message_id
            let mut parts_by_msg: std::collections::HashMap<String, Vec<PartWithId>> =
                std::collections::HashMap::new();
            for (id, message_id, session_id, tc, tu, data) in part_rows {
                if let Ok(part) = serde_json::from_str::<Part>(&data) {
                    parts_by_msg
                        .entry(message_id.clone())
                        .or_default()
                        .push(PartWithId {
                            id,
                            message_id,
                            session_id,
                            part,
                            time_created: tc,
                            time_updated: tu,
                        });
                }
            }

            // Build result
            let mut result = Vec::with_capacity(msg_rows.len());
            for (id, session_id, tc, tu, data) in msg_rows {
                if let Ok(message) = serde_json::from_str::<Message>(&data) {
                    let parts = parts_by_msg.remove(&id).unwrap_or_default();
                    result.push(MessageWithParts {
                        id: id.clone(),
                        session_id,
                        message,
                        parts,
                        time_created: tc,
                        time_updated: tu,
                    });
                }
            }

            Ok(result)
        })
    }

    /// Get a single part by ID.
    pub fn get_part(&self, part_id: &str) -> Result<PartWithId> {
        self.db.use_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, message_id, session_id, time_created, time_updated, data FROM part WHERE id = ?1",
            )?;
            let row = stmt.query_row(rusqlite::params![part_id], |row| {
                let data: String = row.get(5)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    data,
                ))
            })?;
            let part = serde_json::from_str::<Part>(&row.5).context("invalid part data")?;
            Ok(PartWithId {
                id: row.0,
                message_id: row.1,
                session_id: row.2,
                part,
                time_created: row.3,
                time_updated: row.4,
            })
        })
    }

    /// Fork a session: copy all messages and parts to a new session.
    pub fn fork(&self, session_id: &str, project_id: &str, directory: &str) -> Result<Session> {
        let new_session = self.create(project_id, directory, None)?;
        let messages = self.messages(session_id)?;

        self.db.transaction(|conn| {
            // Update parent_id
            conn.execute(
                "UPDATE session SET parent_id = ?1 WHERE id = ?2",
                rusqlite::params![session_id, &new_session.id],
            )?;

            for msg in &messages {
                let new_msg_id = Identifier::ascending(Prefix::Message).into_string();
                let now = Utc::now().timestamp_millis();
                let msg_data = serde_json::to_string(&msg.message)?;

                conn.execute(
                    "INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![&new_msg_id, &new_session.id, now, now, &msg_data],
                )?;

                for part in &msg.parts {
                    let new_part_id = Identifier::ascending(Prefix::Part).into_string();
                    let part_data = serde_json::to_string(&part.part)?;
                    conn.execute(
                        "INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        rusqlite::params![&new_part_id, &new_msg_id, &new_session.id, now, now, &part_data],
                    )?;
                }
            }

            Ok(())
        })?;

        debug!(old_id = session_id, new_id = %new_session.id, "session forked");
        Ok(new_session)
    }

    fn publish_session_updated(&self, session_id: &str, title: &str) {
        self.bus.publish(Event::SessionUpdated(SessionEvent {
            id: session_id.parse().unwrap_or_else(|_| Identifier::create(Prefix::Session)),
            title: title.to_string(),
        }));
    }
}

/// Generate a short random slug for the session URL.
fn generate_slug() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{:x}", ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{AssistantMessage, TextPart, ToolPart, ToolState, UserMessage};

    fn setup() -> (Arc<Database>, SessionService) {
        let db = Database::open_memory().unwrap();
        let bus = Bus::default();
        // Create a project first (foreign key)
        db.use_conn(|conn| {
            let now = Utc::now().timestamp_millis();
            conn.execute(
                "INSERT INTO project (id, worktree, name, time_created, time_updated) VALUES ('prj_test', '/tmp', 'test', ?1, ?2)",
                rusqlite::params![now, now],
            )?;
            Ok(())
        }).unwrap();
        let svc = SessionService::new(db.clone(), bus);
        (db, svc)
    }

    #[test]
    fn create_and_get_session() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp/project", None).unwrap();
        assert_eq!(session.title, "New Session");

        let fetched = svc.get(&session.id).unwrap();
        assert_eq!(fetched.id, session.id);
        assert_eq!(fetched.directory, "/tmp/project");
    }

    #[test]
    fn list_sessions() {
        let (_db, svc) = setup();
        svc.create("prj_test", "/tmp", None).unwrap();
        svc.create("prj_test", "/tmp", None).unwrap();

        let sessions = svc.list("prj_test").unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn remove_session() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp", None).unwrap();
        svc.remove(&session.id).unwrap();

        assert!(svc.get(&session.id).is_err());
    }

    #[test]
    fn set_title() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp", None).unwrap();
        svc.set_title(&session.id, "Updated Title").unwrap();

        let fetched = svc.get(&session.id).unwrap();
        assert_eq!(fetched.title, "Updated Title");
    }

    #[test]
    fn add_message_and_parts() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp", None).unwrap();

        // Add user message
        let msg = Message::User(UserMessage {
            content: "hello".into(),
            images: vec![],
        });
        let msg_id = svc.add_message(&session.id, &msg).unwrap();

        // Add text part
        let part = Part::Text(TextPart {
            content: "Hello! How can I help?".into(),
        });
        svc.add_part(&session.id, &msg_id, &part).unwrap();

        // Fetch messages
        let messages = svc.messages(&session.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].parts.len(), 1);
    }

    #[test]
    fn add_tool_part_lifecycle() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp", None).unwrap();

        let msg = Message::Assistant(AssistantMessage {
            model: "test".into(),
            agent: "build".into(),
            system: "".into(),
        });
        let msg_id = svc.add_message(&session.id, &msg).unwrap();

        // Pending → Running → Completed
        let part = Part::Tool(ToolPart {
            call_id: "call_1".into(),
            tool: "bash".into(),
            state: ToolState::Pending {
                input: serde_json::json!({"command": "ls"}),
                raw: None,
            },
        });
        let part_id = svc.add_part(&session.id, &msg_id, &part).unwrap();

        // Update to completed
        let updated = Part::Tool(ToolPart {
            call_id: "call_1".into(),
            tool: "bash".into(),
            state: ToolState::Completed {
                input: serde_json::json!({"command": "ls"}),
                output: "file1.rs\nfile2.rs".into(),
                title: "bash: ls".into(),
                metadata: serde_json::json!({}),
                time_start: 1000,
                time_end: 1050,
                attachments: None,
            },
        });
        svc.update_part(&part_id, &updated).unwrap();

        let fetched = svc.get_part(&part_id).unwrap();
        assert_eq!(fetched.part.type_name(), "tool");
    }

    #[test]
    fn fork_session() {
        let (_db, svc) = setup();
        let session = svc.create("prj_test", "/tmp", None).unwrap();

        let msg = Message::User(UserMessage {
            content: "hello".into(),
            images: vec![],
        });
        let msg_id = svc.add_message(&session.id, &msg).unwrap();
        svc.add_part(
            &session.id,
            &msg_id,
            &Part::Text(TextPart { content: "world".into() }),
        ).unwrap();

        let forked = svc.fork(&session.id, "prj_test", "/tmp").unwrap();
        let forked_msgs = svc.messages(&forked.id).unwrap();
        assert_eq!(forked_msgs.len(), 1);
        assert_eq!(forked_msgs[0].parts.len(), 1);
    }
}
