//! Session management system.
//!
//! Mirrors `src/session/` from the original OpenCode.
//! Handles message storage, streaming, tool execution, and retry logic.

pub mod compaction;
pub mod message;
pub mod processor;
pub mod retry;
pub mod session;
pub mod share;
pub mod system_prompt;

pub use message::{Message, MessageWithParts, Part, PartWithId};
pub use session::{Session, SessionService};
