//! Typed event bus with instance-scoped subscriptions.
//!
//! Mirrors `src/bus/` from the original OpenCode.
//! Uses tokio broadcast channels for pub/sub.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::LazyLock;
use tokio::sync::broadcast;

use crate::id::{MessageId, PartId, PermissionId, ProjectId, QuestionId, SessionId};

/// All possible bus events in the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    // Session events
    SessionCreated(SessionEvent),
    SessionUpdated(SessionEvent),
    SessionDeleted {
        id: SessionId,
    },

    // Message events
    MessageUpdated {
        session_id: SessionId,
        message_id: MessageId,
    },
    MessageRemoved {
        session_id: SessionId,
        message_id: MessageId,
    },

    // Part events
    PartUpdated {
        session_id: SessionId,
        message_id: MessageId,
        part_id: PartId,
    },
    PartDelta {
        session_id: SessionId,
        message_id: MessageId,
        part_id: PartId,
        field: String,
        delta: String,
    },
    PartRemoved {
        session_id: SessionId,
        message_id: MessageId,
        part_id: PartId,
    },

    // Permission events
    PermissionAsked {
        id: PermissionId,
        session_id: SessionId,
    },
    PermissionReplied {
        session_id: SessionId,
        request_id: PermissionId,
        reply: String,
    },

    // Question events
    QuestionAsked {
        id: QuestionId,
        session_id: SessionId,
    },
    QuestionReplied {
        id: QuestionId,
        session_id: SessionId,
    },

    // File events
    FileEdited {
        path: PathBuf,
    },
    FileWatcherUpdated {
        paths: Vec<PathBuf>,
    },

    // Project events
    ProjectUpdated {
        id: ProjectId,
    },

    // Session status
    SessionStatus {
        session_id: SessionId,
        status: SessionStatusInfo,
    },
    SessionDiff {
        session_id: SessionId,
    },
    SessionError {
        session_id: SessionId,
        error: String,
    },

    // MCP events
    McpToolsChanged {
        name: String,
    },

    // Server events
    ServerConnected,
    ServerDisposed,

    // Instance lifecycle
    InstanceDisposed {
        directory: PathBuf,
    },

    // LSP events
    LspDiagnostics {
        uri: String,
    },

    // Todo events
    TodoUpdated {
        session_id: SessionId,
    },
}

/// Session event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub id: SessionId,
    pub title: String,
}

/// Session status variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionStatusInfo {
    Idle,
    Busy,
    Retry {
        attempt: u32,
        message: String,
        next: u64,
    },
}

/// The local event bus for a single instance.
#[derive(Debug, Clone)]
pub struct Bus {
    tx: broadcast::Sender<Event>,
}

impl Bus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Publish an event to all subscribers.
    pub fn publish(&self, event: Event) {
        // Ignore errors (no subscribers)
        let _ = self.tx.send(event);
    }

    /// Subscribe to all events. Returns a receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Get the sender for direct access (e.g., for Database.effect pattern).
    pub fn sender(&self) -> &broadcast::Sender<Event> {
        &self.tx
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Global bus for cross-instance communication.
/// Events include the source directory.
#[derive(Debug, Clone)]
pub struct GlobalBus {
    tx: broadcast::Sender<(Option<PathBuf>, Event)>,
}

static GLOBAL_BUS: LazyLock<GlobalBus> = LazyLock::new(|| {
    let (tx, _) = broadcast::channel(1024);
    GlobalBus { tx }
});

impl GlobalBus {
    pub fn publish(directory: Option<PathBuf>, event: Event) {
        let _ = GLOBAL_BUS.tx.send((directory, event));
    }

    pub fn subscribe() -> broadcast::Receiver<(Option<PathBuf>, Event)> {
        GLOBAL_BUS.tx.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::{Identifier, Prefix};

    #[tokio::test]
    async fn publish_subscribe() {
        let bus = Bus::new(16);
        let mut rx = bus.subscribe();

        let id = Identifier::create(Prefix::Session);
        bus.publish(Event::SessionCreated(SessionEvent {
            id: id.clone(),
            title: "test".to_string(),
        }));

        let event = rx.recv().await.unwrap();
        match event {
            Event::SessionCreated(e) => assert_eq!(e.id, id),
            _ => panic!("unexpected event"),
        }
    }
}
