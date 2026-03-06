//! Instance-scoped environment variable isolation.
//!
//! Mirrors `src/env/index.ts` from the original OpenCode.
//! Allows per-instance environment overrides without affecting the process.

use dashmap::DashMap;
use std::sync::{Arc, LazyLock};

/// Instance-scoped environment store.
/// Falls back to process environment when no override exists.
#[derive(Debug, Clone)]
pub struct Env {
    overrides: Arc<DashMap<String, String>>,
}

static GLOBAL_ENV: LazyLock<Env> = LazyLock::new(Env::new);

impl Env {
    pub fn new() -> Self {
        Self {
            overrides: Arc::new(DashMap::new()),
        }
    }

    /// Get a value: override first, then process env.
    pub fn get(&self, key: &str) -> Option<String> {
        if let Some(val) = self.overrides.get(key) {
            return Some(val.value().clone());
        }
        std::env::var(key).ok()
    }

    /// Set an instance-level override.
    pub fn set(&self, key: &str, val: &str) {
        self.overrides.insert(key.to_string(), val.to_string());
    }

    /// Remove an instance-level override.
    pub fn remove(&self, key: &str) {
        self.overrides.remove(key);
    }

    /// Get all environment variables (overrides merged with process env).
    pub fn all(&self) -> std::collections::HashMap<String, String> {
        let mut map: std::collections::HashMap<String, String> = std::env::vars().collect();
        for entry in self.overrides.iter() {
            map.insert(entry.key().clone(), entry.value().clone());
        }
        map
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}

/// Access the global (default) environment store.
pub fn global() -> &'static Env {
    &GLOBAL_ENV
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_takes_precedence() {
        let env = Env::new();
        unsafe { std::env::set_var("TEST_OPENCODER_ENV", "original") };
        assert_eq!(env.get("TEST_OPENCODER_ENV").unwrap(), "original");

        env.set("TEST_OPENCODER_ENV", "override");
        assert_eq!(env.get("TEST_OPENCODER_ENV").unwrap(), "override");

        env.remove("TEST_OPENCODER_ENV");
        assert_eq!(env.get("TEST_OPENCODER_ENV").unwrap(), "original");
        unsafe { std::env::remove_var("TEST_OPENCODER_ENV") };
    }
}
