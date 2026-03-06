//! File system watcher for detecting changes.
//!
//! Uses the `notify` crate for cross-platform file watching.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::debug;

/// A file change event.
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: PathBuf,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub enum ChangeKind {
    Create,
    Modify,
    Remove,
}

/// Start watching a directory for file changes.
/// Returns a receiver that emits batched change events.
pub fn watch(dir: &Path) -> anyhow::Result<(mpsc::Receiver<Vec<FileChange>>, WatcherHandle)> {
    let (tx, rx) = mpsc::channel(256);
    let (event_tx, mut event_rx) = mpsc::channel::<FileChange>(1024);

    let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
        if let Ok(event) = res {
            let kind = match event.kind {
                notify::EventKind::Create(_) => ChangeKind::Create,
                notify::EventKind::Modify(_) => ChangeKind::Modify,
                notify::EventKind::Remove(_) => ChangeKind::Remove,
                _ => return,
            };
            for path in event.paths {
                let _ = event_tx.blocking_send(FileChange {
                    path,
                    kind: kind.clone(),
                });
            }
        }
    })?;

    watcher.watch(dir, RecursiveMode::Recursive)?;
    debug!("file watcher started for {}", dir.display());

    // Debounce: batch events every 200ms
    tokio::spawn(async move {
        loop {
            let mut batch = Vec::new();

            // Wait for first event
            match event_rx.recv().await {
                Some(change) => batch.push(change),
                None => break,
            }

            // Collect more events within debounce window
            let deadline = tokio::time::Instant::now() + Duration::from_millis(200);
            loop {
                match tokio::time::timeout_at(deadline, event_rx.recv()).await {
                    Ok(Some(change)) => batch.push(change),
                    _ => break,
                }
            }

            if !batch.is_empty() {
                if tx.send(batch).await.is_err() {
                    break;
                }
            }
        }
    });

    Ok((rx, WatcherHandle { _watcher: watcher }))
}

/// Handle to keep the watcher alive. Drop to stop watching.
pub struct WatcherHandle {
    _watcher: RecommendedWatcher,
}
