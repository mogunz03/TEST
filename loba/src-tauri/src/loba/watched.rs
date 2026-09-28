//! A JSON file the user may edit by hand while the app runs. Reads are
//! served from memory and reloaded only when the file's mtime moves, so
//! checking it on every dictation costs one `stat`.

use loba_core::config::{load_or_seed, write_json};
use serde::{de::DeserializeOwned, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

pub struct Watched<T> {
    path: PathBuf,
    normalize: fn(T) -> T,
    inner: Mutex<(T, Option<SystemTime>)>,
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl<T: Clone + Serialize + DeserializeOwned> Watched<T> {
    pub fn open(path: PathBuf, seed: fn() -> T, normalize: fn(T) -> T) -> Self {
        let value = normalize(load_or_seed(&path, seed));
        let stamp = mtime(&path);
        Self {
            path,
            normalize,
            inner: Mutex::new((value, stamp)),
        }
    }

    pub fn get(&self) -> T {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let now = mtime(&self.path);
        if now.is_some() && now != guard.1 {
            match std::fs::read_to_string(&self.path)
                .map_err(|e| e.to_string())
                .and_then(|raw| serde_json::from_str::<T>(&raw).map_err(|e| e.to_string()))
            {
                Ok(v) => {
                    log::info!("Reloaded {}", self.path.display());
                    guard.0 = (self.normalize)(v);
                }
                // Keep serving the last good value while the user is mid-edit.
                Err(e) => log::warn!("Ignoring invalid {}: {e}", self.path.display()),
            }
            guard.1 = now;
        }
        guard.0.clone()
    }

    pub fn set(&self, next: T) -> Result<T, String> {
        let next = (self.normalize)(next);
        write_json(&self.path, &next).map_err(|e| e.to_string())?;
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        *guard = (next.clone(), mtime(&self.path));
        Ok(next)
    }
}
