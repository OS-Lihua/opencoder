//! Global path constants following XDG Base Directory specification.
//!
//! Mirrors `src/global/index.ts` from the original OpenCode.

use std::path::PathBuf;
use std::sync::LazyLock;

use crate::flag;

const APP_NAME: &str = "opencode";
const CACHE_VERSION: &str = "14";

fn ensure_dir(path: &PathBuf) -> &PathBuf {
    std::fs::create_dir_all(path).ok();
    path
}

/// Global application paths.
pub struct Paths {
    pub home: PathBuf,
    pub data: PathBuf,
    pub bin: PathBuf,
    pub log: PathBuf,
    pub cache: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
}

static PATHS: LazyLock<Paths> = LazyLock::new(|| {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let data = dirs::data_dir()
        .unwrap_or_else(|| home.join(".local/share"))
        .join(APP_NAME);
    let cache = dirs::cache_dir()
        .unwrap_or_else(|| home.join(".cache"))
        .join(APP_NAME);
    let config = dirs::config_dir()
        .unwrap_or_else(|| home.join(".config"))
        .join(APP_NAME);
    let state = dirs::state_dir()
        .unwrap_or_else(|| home.join(".local/state"))
        .join(APP_NAME);

    let paths = Paths {
        home,
        bin: data.join("bin"),
        log: state.join("log"),
        data,
        cache,
        config,
        state,
    };

    // Ensure all directories exist
    ensure_dir(&paths.data);
    ensure_dir(&paths.bin);
    ensure_dir(&paths.log);
    ensure_dir(&paths.cache);
    ensure_dir(&paths.config);
    ensure_dir(&paths.state);

    // Cache version management - clear stale cache
    let version_file = paths.cache.join("version");
    let current = std::fs::read_to_string(&version_file).unwrap_or_default();
    if current.trim() != CACHE_VERSION {
        // Wipe cache and recreate
        if paths.cache.exists() {
            let _ = std::fs::remove_dir_all(&paths.cache);
            let _ = std::fs::create_dir_all(&paths.cache);
        }
        let _ = std::fs::write(&version_file, CACHE_VERSION);
    }

    paths
});

/// Get global application paths.
pub fn paths() -> &'static Paths {
    &PATHS
}

/// Get the database file path.
pub fn db_path() -> PathBuf {
    let custom = flag::get_string("OPENCODE_DB_PATH");
    if let Some(p) = custom {
        return PathBuf::from(p);
    }
    paths().data.join("opencode.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_initialized() {
        let p = paths();
        assert!(!p.home.as_os_str().is_empty());
        assert!(p.data.to_str().unwrap().contains("opencode"));
    }
}
