//! `config.json` — the pipeline's own knobs, kept out of Handy's settings store
//! so the fork's surface stays in one directory the user can read and version.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_MODEL: &str = "qwen3:8b";
pub const DEFAULT_TIMEOUT_MS: u64 = 4_000;
/// Command mode is deliberate (the user is waiting on a rewrite of their own
/// selection), so it gets a longer budget than dictation — but still a cap.
pub const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 15_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
#[serde(default)]
pub struct LobaConfig {
    /// Master switch for the cleanup pass. Off = raw transcript (after the
    /// dictionary) goes straight to paste.
    pub cleanup_enabled: bool,
    pub ollama_url: String,
    pub model: String,
    /// Hard ceiling for the cleanup round trip. Past it, the raw transcript
    /// is pasted and the LLM answer is discarded.
    pub timeout_ms: u64,
    pub command_timeout_ms: u64,
    /// Keep the model resident this long after each call (Ollama `keep_alive`).
    /// A cold 8B load alone can blow the 4 s budget.
    pub keep_alive: String,
}

impl Default for LobaConfig {
    fn default() -> Self {
        Self {
            cleanup_enabled: true,
            ollama_url: DEFAULT_OLLAMA_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            command_timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
            keep_alive: "30m".to_string(),
        }
    }
}

impl LobaConfig {
    /// Clamp values a hand-edited file could get wrong, and refuse anything
    /// that is not loopback: L.O.B.A never talks to the network.
    pub fn sanitized(mut self) -> Self {
        let d = Self::default();
        if !is_loopback_url(&self.ollama_url) {
            log::warn!(
                "Ignoring non-loopback ollama_url '{}'; L.O.B.A is offline-only",
                self.ollama_url
            );
            self.ollama_url = d.ollama_url;
        }
        self.ollama_url = self.ollama_url.trim_end_matches('/').to_string();
        if self.model.trim().is_empty() {
            self.model = d.model;
        }
        self.timeout_ms = self.timeout_ms.clamp(500, 30_000);
        self.command_timeout_ms = self.command_timeout_ms.clamp(1_000, 120_000);
        if self.keep_alive.trim().is_empty() {
            self.keep_alive = d.keep_alive;
        }
        self
    }
}

pub fn is_loopback_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    let Some(rest) = rest else { return false };
    let authority = rest.split('/').next().unwrap_or("");
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Read a JSON file, or write `default` there and return it. A malformed file
/// is never overwritten — the user's edits win; we log and run on defaults.
pub fn load_or_seed<T>(path: &Path, default: impl FnOnce() -> T) -> T
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(e) => {
                log::error!("{} is not valid ({e}); using defaults", path.display());
                default()
            }
        },
        Err(_) => {
            let v = default();
            if let Err(e) = write_json(path, &v) {
                log::warn!("Could not seed {}: {e}", path.display());
            }
            v
        }
    }
}

/// Atomic write: temp file + rename, so a crash mid-save never leaves the user
/// with a truncated dictionary.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, body + "\n")?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_only() {
        assert!(is_loopback_url("http://127.0.0.1:11434"));
        assert!(is_loopback_url("http://localhost:11434/"));
        assert!(is_loopback_url("http://[::1]:11434"));
        assert!(!is_loopback_url("http://192.168.1.4:11434"));
        assert!(!is_loopback_url("https://api.example.com"));
        assert!(!is_loopback_url("127.0.0.1:11434"));
    }

    #[test]
    fn sanitize_rejects_remote_and_clamps() {
        let c = LobaConfig {
            ollama_url: "http://10.0.0.2:11434".into(),
            timeout_ms: 5,
            model: " ".into(),
            ..Default::default()
        }
        .sanitized();
        assert_eq!(c.ollama_url, DEFAULT_OLLAMA_URL);
        assert_eq!(c.timeout_ms, 500);
        assert_eq!(c.model, DEFAULT_MODEL);
    }

    #[test]
    fn partial_file_fills_defaults() {
        let c: LobaConfig = serde_json::from_str(r#"{"model":"qwen3:4b"}"#).unwrap();
        assert_eq!(c.model, "qwen3:4b");
        assert_eq!(c.timeout_ms, DEFAULT_TIMEOUT_MS);
    }

    #[test]
    fn malformed_file_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("config.json");
        std::fs::write(&p, "{ nope").unwrap();
        let c: LobaConfig = load_or_seed(&p, LobaConfig::default);
        assert_eq!(c, LobaConfig::default());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ nope");
    }
}
