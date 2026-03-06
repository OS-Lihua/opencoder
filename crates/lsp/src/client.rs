//! LSP client for communicating with language servers.
//!
//! Manages multiple language server connections with lazy initialization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::{Mutex, oneshot};
use tracing::{info, warn};

use crate::languages;

/// Manages multiple LSP server connections.
pub struct LspManager {
    root_dir: PathBuf,
    clients: Mutex<HashMap<String, Arc<LspClient>>>,
}

impl LspManager {
    pub fn new(root_dir: &Path) -> Self {
        Self {
            root_dir: root_dir.to_path_buf(),
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Get or lazily create an LSP client for the given language.
    pub async fn client_for_language(&self, language: &str) -> Result<Arc<LspClient>> {
        let mut clients = self.clients.lock().await;

        if let Some(client) = clients.get(language) {
            return Ok(client.clone());
        }

        let servers = languages::language_servers();
        let server_info = servers
            .get(language)
            .ok_or_else(|| anyhow::anyhow!("no LSP server configured for {language}"))?;

        let client = LspClient::start(
            language,
            server_info.command,
            server_info.args,
            &self.root_dir,
        )
        .await?;

        let client = Arc::new(client);
        clients.insert(language.to_string(), client.clone());
        Ok(client)
    }

    /// Get an LSP client for a file path (auto-detects language).
    pub async fn client_for_file(&self, path: &Path) -> Result<Arc<LspClient>> {
        let language = languages::language_for_path(path)
            .ok_or_else(|| anyhow::anyhow!("unknown language for {:?}", path))?;
        self.client_for_language(language).await
    }

    /// Shutdown all LSP clients.
    pub async fn shutdown_all(&self) {
        let mut clients = self.clients.lock().await;
        for (lang, client) in clients.drain() {
            if let Err(e) = client.shutdown().await {
                warn!(language = %lang, error = %e, "LSP shutdown error");
            }
        }
    }
}

/// An LSP client connected to a single language server.
pub struct LspClient {
    language: String,
    writer: Mutex<tokio::process::ChildStdin>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl LspClient {
    /// Start a language server process and initialize the LSP connection.
    async fn start(language: &str, command: &str, args: &[&str], root_dir: &Path) -> Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to start LSP server: {command}"))?;

        let stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<serde_json::Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Reader task: parse LSP Content-Length framed messages
        let pending_clone = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                // Read headers
                let mut content_length: usize = 0;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) => return,
                        Ok(_) => {}
                        Err(_) => return,
                    }
                    let line = line.trim();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(len) = line.strip_prefix("Content-Length: ") {
                        content_length = len.parse().unwrap_or(0);
                    }
                }

                if content_length == 0 {
                    continue;
                }

                // Read body
                let mut body = vec![0u8; content_length];
                if tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
                    .await
                    .is_err()
                {
                    return;
                }

                let body_str = String::from_utf8_lossy(&body);
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&body_str)
                    && let Some(id) = msg.get("id").and_then(|v| v.as_u64())
                    && msg.get("method").is_none()
                {
                    // This is a response
                    let result = msg
                        .get("result")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let mut pending = pending_clone.lock().await;
                    if let Some(tx) = pending.remove(&id) {
                        let _ = tx.send(result);
                    }
                }
            }
        });

        let client = Self {
            language: language.to_string(),
            writer: Mutex::new(stdin),
            pending,
            next_id: std::sync::atomic::AtomicU64::new(1),
        };

        // Initialize
        let root_uri = format!("file://{}", root_dir.display());
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "definition": {"dynamicRegistration": false},
                    "references": {"dynamicRegistration": false},
                    "hover": {"contentFormat": ["plaintext", "markdown"]},
                    "completion": {"completionItem": {"snippetSupport": false}}
                }
            }
        });

        client.request("initialize", init_params).await?;
        client.notify("initialized", serde_json::json!({})).await?;

        info!(language, command, "LSP server started");
        Ok(client)
    }

    /// Send an LSP request and wait for the response.
    pub async fn request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let id = self
            .next_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let body = serde_json::to_string(&msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        {
            let mut writer = self.writer.lock().await;
            writer
                .write_all(frame.as_bytes())
                .await
                .context("LSP write failed")?;
            writer.flush().await?;
        }

        let result = tokio::time::timeout(std::time::Duration::from_secs(10), rx)
            .await
            .context("LSP request timeout")?
            .context("LSP response channel closed")?;

        Ok(result)
    }

    /// Send an LSP notification (no response).
    pub async fn notify(&self, method: &str, params: serde_json::Value) -> Result<()> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let body = serde_json::to_string(&msg)?;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);

        let mut writer = self.writer.lock().await;
        writer.write_all(frame.as_bytes()).await?;
        writer.flush().await?;
        Ok(())
    }

    /// Get definition for a position in a file.
    pub async fn definition(
        &self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<serde_json::Value> {
        self.request(
            "textDocument/definition",
            serde_json::json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        )
        .await
    }

    /// Get references for a position in a file.
    pub async fn references(
        &self,
        uri: &str,
        line: u32,
        character: u32,
    ) -> Result<serde_json::Value> {
        self.request(
            "textDocument/references",
            serde_json::json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character},
                "context": {"includeDeclaration": true}
            }),
        )
        .await
    }

    /// Get hover info for a position in a file.
    pub async fn hover(&self, uri: &str, line: u32, character: u32) -> Result<serde_json::Value> {
        self.request(
            "textDocument/hover",
            serde_json::json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        )
        .await
    }

    /// Get document symbols.
    pub async fn document_symbols(&self, uri: &str) -> Result<serde_json::Value> {
        self.request(
            "textDocument/documentSymbol",
            serde_json::json!({"textDocument": {"uri": uri}}),
        )
        .await
    }

    /// Shutdown the LSP server.
    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.request("shutdown", serde_json::json!(null)).await;
        self.notify("exit", serde_json::json!(null)).await?;
        Ok(())
    }

    pub fn language(&self) -> &str {
        &self.language
    }
}
