//! File system operations and watching.
//!
//! Mirrors `src/file/` from the original OpenCode.
//! Provides file reading with binary detection, directory listing,
//! git status integration, and file watching.

pub mod detect;
pub mod formatter;
pub mod listing;
pub mod watcher;

pub use detect::is_binary;
pub use listing::{list_files, FileEntry};
