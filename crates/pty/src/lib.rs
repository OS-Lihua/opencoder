//! PTY (pseudo-terminal) management.
//!
//! Mirrors `src/pty/index.ts` from the original OpenCode.
//! Manages PTY sessions with buffered output and WebSocket connectivity.

use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc};
use tracing::debug;

use opencoder_core::id::{Identifier, Prefix};

/// Maximum buffer size per PTY session (2MB).
const MAX_BUFFER_SIZE: usize = 2 * 1024 * 1024;

/// PTY session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtyInfo {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub running: bool,
}

/// A PTY session with buffered output.
pub struct PtySession {
    info: PtyInfo,
    child: Option<Child>,
    buffer: VecDeque<u8>,
    output_tx: mpsc::Sender<Vec<u8>>,
    output_rx: Option<mpsc::Receiver<Vec<u8>>>,
    stdin_tx: Option<mpsc::Sender<Vec<u8>>>,
}

impl PtySession {
    /// Create and start a new PTY session.
    pub async fn create(
        command: &str,
        args: &[String],
        cwd: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let id = Identifier::create(Prefix::Pty).into_string();

        let (output_tx, output_rx) = mpsc::channel(1024);
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(256);

        let mut child = Command::new(command)
            .args(args)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to spawn PTY: {command}"))?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdin = child.stdin.take();

        // Stdout reader
        let tx1 = output_tx.clone();
        if let Some(stdout) = stdout {
            tokio::spawn(async move {
                let mut stdout = stdout;
                let mut buf = [0u8; 4096];
                loop {
                    match stdout.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx1.send(buf[..n].to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Stderr reader
        let tx2 = output_tx.clone();
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut stderr = stderr;
                let mut buf = [0u8; 4096];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => break,
                        Ok(n) => {
                            if tx2.send(buf[..n].to_vec()).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        // Stdin writer
        if let Some(mut stdin) = stdin {
            tokio::spawn(async move {
                while let Some(data) = stdin_rx.recv().await {
                    if stdin.write_all(&data).await.is_err() {
                        break;
                    }
                    if stdin.flush().await.is_err() {
                        break;
                    }
                }
            });
        }

        debug!(id = %id, command, "PTY session created");

        Ok(Self {
            info: PtyInfo {
                id,
                command: command.to_string(),
                args: args.to_vec(),
                cwd: cwd.to_string(),
                cols,
                rows,
                running: true,
            },
            child: Some(child),
            buffer: VecDeque::with_capacity(MAX_BUFFER_SIZE),
            output_tx,
            output_rx: Some(output_rx),
            stdin_tx: Some(stdin_tx),
        })
    }

    /// Write data to the PTY's stdin.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if let Some(ref tx) = self.stdin_tx {
            tx.send(data.to_vec()).await.context("PTY stdin closed")?;
        }
        Ok(())
    }

    /// Get the session info.
    pub fn info(&self) -> &PtyInfo {
        &self.info
    }

    /// Get the session ID.
    pub fn id(&self) -> &str {
        &self.info.id
    }

    /// Take the output receiver (can only be taken once).
    pub fn take_output_rx(&mut self) -> Option<mpsc::Receiver<Vec<u8>>> {
        self.output_rx.take()
    }

    /// Check if the process is still running.
    pub async fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    self.info.running = false;
                    false
                }
                Ok(None) => true,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Kill the PTY process.
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(ref mut child) = self.child {
            child.kill().await.ok();
            self.info.running = false;
        }
        Ok(())
    }
}

/// Manages multiple PTY sessions.
pub struct PtyManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<PtySession>>>>>,
}

impl PtyManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new PTY session.
    pub async fn create(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        cols: u16,
        rows: u16,
    ) -> Result<String> {
        let session = PtySession::create(command, args, cwd, cols, rows).await?;
        let id = session.id().to_string();
        let mut sessions = self.sessions.lock().await;
        sessions.insert(id.clone(), Arc::new(Mutex::new(session)));
        Ok(id)
    }

    /// Get a PTY session by ID.
    pub async fn get(&self, id: &str) -> Option<Arc<Mutex<PtySession>>> {
        let sessions = self.sessions.lock().await;
        sessions.get(id).cloned()
    }

    /// List all PTY sessions.
    pub async fn list(&self) -> Vec<PtyInfo> {
        let sessions = self.sessions.lock().await;
        let mut infos = Vec::new();
        for session in sessions.values() {
            let s = session.lock().await;
            infos.push(s.info().clone());
        }
        infos
    }

    /// Kill and remove a PTY session.
    pub async fn remove(&self, id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(id) {
            let mut s = session.lock().await;
            s.kill().await?;
        }
        Ok(())
    }
}

impl Default for PtyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_pty_session() {
        let session = PtySession::create("echo", &["hello".to_string()], "/tmp", 80, 24).await;
        assert!(session.is_ok());
        let session = session.unwrap();
        assert_eq!(session.info().command, "echo");
        assert_eq!(session.info().cols, 80);
    }

    #[tokio::test]
    async fn pty_manager_lifecycle() {
        let manager = PtyManager::new();
        let id = manager
            .create("echo", &["test".to_string()], "/tmp", 80, 24)
            .await
            .unwrap();

        let sessions = manager.list().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, id);

        manager.remove(&id).await.unwrap();
        let sessions = manager.list().await;
        assert_eq!(sessions.len(), 0);
    }

    #[tokio::test]
    async fn pty_output() {
        let mut session = PtySession::create("echo", &["hello world".to_string()], "/tmp", 80, 24)
            .await
            .unwrap();

        let mut rx = session.take_output_rx().unwrap();

        // Wait for output
        let output = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await;
        assert!(output.is_ok());
        if let Ok(Some(data)) = output {
            let text = String::from_utf8_lossy(&data);
            assert!(text.contains("hello world"));
        }
    }
}
