//! Project and instance management.
//!
//! Mirrors `src/project/` from the original OpenCode.
//! Per-directory project context with lazy initialization.

pub mod project;

pub use project::{Project, ProjectService};
