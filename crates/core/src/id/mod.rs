//! Sortable identifier generation with type-safe prefixes.
//!
//! Mirrors `src/id/id.ts` from the original OpenCode.
//! Generates monotonically increasing, sortable IDs with a prefix.
//! Format: `{prefix}_{base62(timestamp)}_{base62(counter)}`

use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Base62 character set (0-9, A-Z, a-z)
const BASE62_CHARS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// Maximum timestamp value for descending IDs
const MAX_TS: u64 = 281_474_976_710_655; // 2^48 - 1

fn encode_base62(mut val: u64, min_len: usize) -> String {
    if val == 0 {
        return "0".repeat(min_len.max(1));
    }
    let mut buf = Vec::with_capacity(12);
    while val > 0 {
        buf.push(BASE62_CHARS[(val % 62) as usize]);
        val /= 62;
    }
    buf.reverse();
    let s = String::from_utf8(buf).unwrap();
    if s.len() < min_len {
        format!("{}{s}", "0".repeat(min_len - s.len()))
    } else {
        s
    }
}

fn decode_base62(s: &str) -> Option<u64> {
    let mut val: u64 = 0;
    for b in s.bytes() {
        let digit = match b {
            b'0'..=b'9' => (b - b'0') as u64,
            b'A'..=b'Z' => (b - b'A' + 10) as u64,
            b'a'..=b'z' => (b - b'a' + 36) as u64,
            _ => return None,
        };
        val = val.checked_mul(62)?.checked_add(digit)?;
    }
    Some(val)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Known identifier prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Prefix {
    #[serde(rename = "ses")]
    Session,
    #[serde(rename = "msg")]
    Message,
    #[serde(rename = "prt")]
    Part,
    #[serde(rename = "per")]
    Permission,
    #[serde(rename = "que")]
    Question,
    #[serde(rename = "usr")]
    User,
    #[serde(rename = "pty")]
    Pty,
    #[serde(rename = "tool")]
    Tool,
    #[serde(rename = "wrk")]
    Workspace,
    #[serde(rename = "prj")]
    Project,
}

impl Prefix {
    pub fn as_str(&self) -> &'static str {
        match self {
            Prefix::Session => "ses",
            Prefix::Message => "msg",
            Prefix::Part => "prt",
            Prefix::Permission => "per",
            Prefix::Question => "que",
            Prefix::User => "usr",
            Prefix::Pty => "pty",
            Prefix::Tool => "tool",
            Prefix::Workspace => "wrk",
            Prefix::Project => "prj",
        }
    }
}

/// A typed, sortable identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Identifier {
    raw: String,
}

impl Identifier {
    /// Create a new ascending identifier with the given prefix.
    pub fn ascending(prefix: Prefix) -> Self {
        let ts = now_ms();
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let raw = format!(
            "{}_{}_{}",
            prefix.as_str(),
            encode_base62(ts, 8),
            encode_base62(count, 4)
        );
        Self { raw }
    }

    /// Create a new descending identifier (newest sorts first).
    pub fn descending(prefix: Prefix) -> Self {
        let ts = MAX_TS - now_ms();
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let raw = format!(
            "{}_{}_{}",
            prefix.as_str(),
            encode_base62(ts, 8),
            encode_base62(count, 4)
        );
        Self { raw }
    }

    /// Alias for `ascending`.
    pub fn create(prefix: Prefix) -> Self {
        Self::ascending(prefix)
    }

    /// Extract the timestamp (milliseconds since epoch) from an ascending ID.
    pub fn timestamp(&self) -> Option<u64> {
        let ts_part = self.raw.split('_').nth(1)?;
        decode_base62(ts_part)
    }

    /// Get the raw string representation.
    pub fn as_str(&self) -> &str {
        &self.raw
    }

    /// Consume and return the inner string.
    pub fn into_string(self) -> String {
        self.raw
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

impl FromStr for Identifier {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Validate basic format: prefix_base62_base62
        let parts: Vec<&str> = s.split('_').collect();
        if parts.len() < 3 {
            return Err(IdError::InvalidFormat(s.to_string()));
        }
        Ok(Self { raw: s.to_string() })
    }
}

impl Serialize for Identifier {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.raw)
    }
}

impl<'de> Deserialize<'de> for Identifier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(serde::de::Error::custom)
    }
}

/// Convenience type aliases for typed IDs.
pub type SessionId = Identifier;
pub type MessageId = Identifier;
pub type PartId = Identifier;
pub type PermissionId = Identifier;
pub type QuestionId = Identifier;
pub type PtyId = Identifier;
pub type WorkspaceId = Identifier;
pub type ProjectId = Identifier;

#[derive(Debug, thiserror::Error)]
pub enum IdError {
    #[error("invalid identifier format: {0}")]
    InvalidFormat(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascending_ids_are_monotonic() {
        let a = Identifier::ascending(Prefix::Session);
        let b = Identifier::ascending(Prefix::Session);
        assert!(b.as_str() > a.as_str());
    }

    #[test]
    fn descending_ids_are_reverse() {
        let a = Identifier::descending(Prefix::Message);
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = Identifier::descending(Prefix::Message);
        // For descending, newer timestamps produce smaller base62 values,
        // so newer IDs sort BEFORE older ones (that's the point).
        assert!(b.as_str() < a.as_str(), "b={} should be < a={}", b, a);
    }

    #[test]
    fn roundtrip_parse() {
        let id = Identifier::create(Prefix::Session);
        let parsed: Identifier = id.as_str().parse().unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn timestamp_extraction() {
        let before = now_ms();
        let id = Identifier::ascending(Prefix::Part);
        let after = now_ms();
        let ts = id.timestamp().unwrap();
        assert!(ts >= before && ts <= after);
    }

    #[test]
    fn serde_roundtrip() {
        let id = Identifier::create(Prefix::Session);
        let json = serde_json::to_string(&id).unwrap();
        let parsed: Identifier = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn base62_encode_decode() {
        for val in [0, 1, 61, 62, 1000, u64::MAX / 2] {
            let encoded = encode_base62(val, 1);
            let decoded = decode_base62(&encoded).unwrap();
            assert_eq!(val, decoded, "failed for {val}");
        }
    }
}
