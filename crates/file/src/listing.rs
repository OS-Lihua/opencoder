//! File listing with gitignore support.
//!
//! Uses the `ignore` crate for efficient .gitignore-aware directory traversal.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

/// A file entry with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub relative_path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
}

/// List files in a directory, respecting .gitignore rules.
/// Returns files sorted by modification time (newest first).
pub fn list_files(dir: &Path, max_results: usize) -> anyhow::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    let walker = WalkBuilder::new(dir)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        // Skip the root directory itself
        if path == dir {
            continue;
        }

        // Skip .git directory
        if path
            .components()
            .any(|c| c.as_os_str() == ".git")
        {
            continue;
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let relative = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        entries.push(FileEntry {
            path: path.to_path_buf(),
            relative_path: relative,
            is_dir: metadata.is_dir(),
            size: metadata.len(),
            modified,
        });

        if entries.len() >= max_results {
            break;
        }
    }

    // Sort by modification time, newest first
    entries.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(entries)
}

/// Get the git status of files in a directory.
pub fn git_status(dir: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "-uall"])
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut results = Vec::new();

    for line in stdout.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = line[..2].trim().to_string();
        let path = line[3..].to_string();
        results.push((path, status));
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn list_files_basic() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), "hello").unwrap();
        fs::write(dir.path().join("b.rs"), "world").unwrap();

        let files = list_files(dir.path(), 100).unwrap();
        // Should find both files (may also include directories)
        let file_names: Vec<_> = files.iter().map(|f| f.relative_path.as_str()).collect();
        assert!(file_names.contains(&"a.txt"));
        assert!(file_names.contains(&"b.rs"));
    }

    #[test]
    fn list_files_max_results() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("file{i}.txt")), "x").unwrap();
        }

        let files = list_files(dir.path(), 5).unwrap();
        assert!(files.len() <= 5);
    }
}
