//! Shell detection and process management.
//!
//! Mirrors `src/shell/shell.ts` from the original OpenCode.

use std::path::PathBuf;

/// Get the preferred shell for the current platform.
/// Priority: zsh > bash > sh (Unix), cmd > powershell (Windows).
pub fn preferred() -> PathBuf {
    #[cfg(unix)]
    {
        for shell in &["/bin/zsh", "/usr/bin/zsh", "/bin/bash", "/usr/bin/bash", "/bin/sh"] {
            if std::path::Path::new(shell).exists() {
                return PathBuf::from(shell);
            }
        }
        if let Ok(shell) = std::env::var("SHELL") {
            return PathBuf::from(shell);
        }
        PathBuf::from("/bin/sh")
    }

    #[cfg(windows)]
    {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            return PathBuf::from(comspec);
        }
        PathBuf::from("cmd.exe")
    }
}

/// List all acceptable shells on this system.
pub fn acceptable() -> Vec<PathBuf> {
    let mut shells = Vec::new();

    #[cfg(unix)]
    {
        for shell in &["/bin/zsh", "/usr/bin/zsh", "/bin/bash", "/usr/bin/bash", "/bin/sh"] {
            if std::path::Path::new(shell).exists() {
                shells.push(PathBuf::from(shell));
            }
        }
    }

    #[cfg(windows)]
    {
        if let Ok(comspec) = std::env::var("COMSPEC") {
            shells.push(PathBuf::from(comspec));
        }
        shells.push(PathBuf::from("powershell.exe"));
    }

    shells
}

/// Get the shell name (e.g., "zsh", "bash") from a path.
pub fn shell_name(path: &std::path::Path) -> &str {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sh")
}

/// Kill a process tree. Sends SIGTERM, then SIGKILL after a short delay.
#[cfg(unix)]
pub fn kill_tree(pid: u32) -> anyhow::Result<()> {
    use std::time::Duration;

    let neg_pid = -(pid as i32);
    unsafe {
        libc::kill(neg_pid, libc::SIGTERM);
    }

    std::thread::sleep(Duration::from_millis(100));

    unsafe {
        libc::kill(neg_pid, libc::SIGKILL);
    }

    Ok(())
}

#[cfg(windows)]
pub fn kill_tree(pid: u32) -> anyhow::Result<()> {
    std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .output()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferred_shell_exists() {
        let shell = preferred();
        assert!(shell.exists(), "preferred shell {:?} does not exist", shell);
    }

    #[test]
    fn acceptable_shells_non_empty() {
        let shells = acceptable();
        assert!(!shells.is_empty());
    }

    #[test]
    fn shell_name_extraction() {
        assert_eq!(shell_name(std::path::Path::new("/bin/zsh")), "zsh");
        assert_eq!(shell_name(std::path::Path::new("/usr/bin/bash")), "bash");
    }
}
