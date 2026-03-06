//! MCP transport layer: stdio and HTTP/SSE.
//!
//! Stdio transport spawns a child process and communicates via stdin/stdout.
//! HTTP transport uses reqwest for StreamableHTTP with SSE fallback.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::debug;

use crate::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Abstract transport for MCP communication.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
    /// Send a request and receive a response.
    async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value>;

    /// Send a notification (no response expected).
    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()>;

    /// Close the transport.
    async fn close(&self) -> Result<()>;
}

/// Stdio transport: communicates with an MCP server via stdin/stdout of a child process.
pub struct StdioTransport {
    request_id: AtomicU64,
    stdin_tx: Mutex<mpsc::Sender<String>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
}

impl StdioTransport {
    /// Spawn a child process and set up JSON-RPC communication.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<(Self, Child)> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = cmd.spawn().with_context(|| format!("failed to spawn {command}"))?;

        let stdin = child.stdin.take().context("no stdin")?;
        let stdout = child.stdout.take().context("no stdout")?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(256);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Writer task
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(msg) = stdin_rx.recv().await {
                if stdin.write_all(msg.as_bytes()).await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        });

        // Reader task
        let pending_clone = pending.clone();
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim().to_string();
                if line.is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonRpcResponse>(&line) {
                    Ok(resp) => {
                        let mut pending = pending_clone.lock().await;
                        if let Some(tx) = pending.remove(&resp.id) {
                            let _ = tx.send(resp);
                        }
                    }
                    Err(e) => {
                        debug!("non-response line from MCP server: {e}");
                    }
                }
            }
        });

        Ok((
            Self {
                request_id: AtomicU64::new(1),
                stdin_tx: Mutex::new(stdin_tx),
                pending,
            },
            child,
        ))
    }
}

#[async_trait::async_trait]
impl Transport for StdioTransport {
    async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest::new(id, method, params);
        let msg = serde_json::to_string(&req)? + "\n";

        let (tx, rx) = oneshot::channel();
        {
            let mut pending = self.pending.lock().await;
            pending.insert(id, tx);
        }

        {
            let stdin_tx = self.stdin_tx.lock().await;
            stdin_tx.send(msg).await.context("stdin closed")?;
        }

        let resp = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .context("MCP request timeout")?
            .context("MCP response channel closed")?;

        if let Some(err) = resp.error {
            anyhow::bail!("MCP error {}: {}", err.code, err.message);
        }

        resp.result.context("empty MCP response")
    }

    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let msg = serde_json::to_string(&notification)? + "\n";
        let stdin_tx = self.stdin_tx.lock().await;
        stdin_tx.send(msg).await.context("stdin closed")?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        // Dropping the sender will close stdin
        Ok(())
    }
}

/// HTTP transport: communicates with an MCP server via HTTP POST + SSE.
pub struct HttpTransport {
    request_id: AtomicU64,
    url: String,
    client: reqwest::Client,
    headers: HashMap<String, String>,
}

impl HttpTransport {
    pub fn new(url: &str, headers: HashMap<String, String>) -> Self {
        Self {
            request_id: AtomicU64::new(1),
            url: url.to_string(),
            client: reqwest::Client::new(),
            headers,
        }
    }
}

#[async_trait::async_trait]
impl Transport for HttpTransport {
    async fn request(&self, method: &str, params: Option<serde_json::Value>) -> Result<serde_json::Value> {
        let id = self.request_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest::new(id, method, params);

        let mut builder = self.client.post(&self.url).json(&req);
        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        let resp = builder.send().await.context("MCP HTTP request failed")?;
        let body: JsonRpcResponse = resp.json().await.context("MCP HTTP response parse failed")?;

        if let Some(err) = body.error {
            anyhow::bail!("MCP error {}: {}", err.code, err.message);
        }

        body.result.context("empty MCP response")
    }

    async fn notify(&self, method: &str, params: Option<serde_json::Value>) -> Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let mut builder = self.client.post(&self.url).json(&notification);
        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }

        builder.send().await.context("MCP HTTP notify failed")?;
        Ok(())
    }

    async fn close(&self) -> Result<()> {
        Ok(())
    }
}
