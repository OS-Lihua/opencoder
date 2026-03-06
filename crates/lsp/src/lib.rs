//! LSP (Language Server Protocol) client integration.
//!
//! Mirrors `src/lsp/` from the original OpenCode.
//! Manages multiple LSP server connections, routed by file extension.

pub mod client;
pub mod languages;

pub use client::{LspClient, LspManager};
