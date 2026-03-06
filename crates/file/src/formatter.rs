//! Auto-formatting system.
//!
//! Detects and runs formatters for edited files.

use std::path::Path;
use std::process::Stdio;

use anyhow::Result;
use tracing::{debug, warn};

/// A formatter definition.
pub struct FormatterDef {
    pub name: &'static str,
    pub command: &'static str,
    pub args: &'static [&'static str],
    pub extensions: &'static [&'static str],
}

/// Built-in formatters.
const FORMATTERS: &[FormatterDef] = &[
    FormatterDef {
        name: "rustfmt",
        command: "rustfmt",
        args: &[],
        extensions: &["rs"],
    },
    FormatterDef {
        name: "prettier",
        command: "npx",
        args: &["prettier", "--write"],
        extensions: &["js", "jsx", "ts", "tsx", "json", "css", "scss", "md", "yaml", "yml"],
    },
    FormatterDef {
        name: "gofmt",
        command: "gofmt",
        args: &["-w"],
        extensions: &["go"],
    },
    FormatterDef {
        name: "black",
        command: "black",
        args: &["-q"],
        extensions: &["py"],
    },
    FormatterDef {
        name: "clang-format",
        command: "clang-format",
        args: &["-i"],
        extensions: &["c", "cpp", "cc", "h", "hpp"],
    },
    FormatterDef {
        name: "shfmt",
        command: "shfmt",
        args: &["-w"],
        extensions: &["sh", "bash"],
    },
];

/// Find a formatter for the given file path.
pub fn find(path: &Path) -> Option<&'static FormatterDef> {
    let ext = path.extension()?.to_str()?;
    FORMATTERS.iter().find(|f| f.extensions.contains(&ext))
}

/// Check if a formatter command exists on the system.
fn command_exists(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a formatter on a file.
pub async fn run(path: &Path) -> Result<bool> {
    let Some(formatter) = find(path) else {
        return Ok(false);
    };

    if !command_exists(formatter.command) {
        debug!(formatter = formatter.name, "formatter not found, skipping");
        return Ok(false);
    }

    let path_str = path.to_string_lossy();
    let mut cmd = tokio::process::Command::new(formatter.command);
    for arg in formatter.args {
        cmd.arg(arg);
    }
    cmd.arg(path_str.as_ref());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());

    let output = cmd.output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            formatter = formatter.name,
            path = %path.display(),
            stderr = %stderr.trim(),
            "formatter failed"
        );
        return Ok(false);
    }

    debug!(formatter = formatter.name, path = %path.display(), "formatted");
    Ok(true)
}

/// Start a listener that formats files on edit events.
pub fn start_format_listener(bus: &opencoder_core::bus::Bus) {
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(opencoder_core::bus::Event::FileEdited { path }) => {
                    if let Err(e) = run(&path).await {
                        warn!(path = %path.display(), error = %e, "format error");
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                _ => {}
            }
        }
    });
}
