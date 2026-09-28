//! Tauri wiring for the L.O.B.A pipeline (logic lives in `loba-core`).
//!
//! Seam: `actions::process_transcription_output` calls [`process`] between
//! transcription and paste. Hotkey, capture and injection code is untouched.

pub mod commands;

use loba_core::cleanup::{self, CleanupJob, Outcome};
use loba_core::config::{load_or_seed, write_json};
use loba_core::prompt::PromptContext;
use loba_core::{LobaConfig, OllamaClient};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub struct LobaState {
    dir: PathBuf,
    config: RwLock<LobaConfig>,
    client: RwLock<OllamaClient>,
}

impl LobaState {
    fn config_path(&self) -> PathBuf {
        self.dir.join("config.json")
    }

    pub fn config(&self) -> LobaConfig {
        self.config.read().map(|c| c.clone()).unwrap_or_default()
    }

    pub fn client(&self) -> OllamaClient {
        self.client
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|_| OllamaClient::new(loba_core::config::DEFAULT_OLLAMA_URL))
    }

    pub fn set_config(&self, next: LobaConfig) -> Result<LobaConfig, String> {
        let next = next.sanitized();
        write_json(&self.config_path(), &next).map_err(|e| e.to_string())?;
        if let Ok(mut c) = self.client.write() {
            if c.base_url() != next.ollama_url {
                *c = OllamaClient::new(&next.ollama_url);
            }
        }
        if let Ok(mut c) = self.config.write() {
            *c = next.clone();
        }
        Ok(next)
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

/// Load (or seed) `<app data>/loba/*.json` and register the state.
pub fn init(app: &AppHandle) {
    let dir = match crate::portable::app_data_dir(app) {
        Ok(d) => d.join("loba"),
        Err(e) => {
            log::error!("L.O.B.A disabled: no app data dir ({e})");
            return;
        }
    };
    let config: LobaConfig =
        load_or_seed(&dir.join("config.json"), LobaConfig::default).sanitized();
    log::info!(
        "L.O.B.A cleanup {} (model {}, budget {} ms, data {})",
        if config.cleanup_enabled { "on" } else { "off" },
        config.model,
        config.timeout_ms,
        dir.display()
    );
    let client = OllamaClient::new(&config.ollama_url);
    app.manage(Arc::new(LobaState {
        dir,
        config: RwLock::new(config),
        client: RwLock::new(client),
    }));
    prewarm(app);
}

fn state(app: &AppHandle) -> Option<Arc<LobaState>> {
    app.try_state::<Arc<LobaState>>().map(|s| Arc::clone(&s))
}

/// True when dictations will go through the cleanup pass (drives the
/// "processing" overlay state).
pub fn cleanup_active(app: &AppHandle) -> bool {
    state(app).is_some_and(|s| s.config().cleanup_enabled)
}

/// Fire-and-forget model load. Called on hotkey press so the 8B weights are
/// resident by the time speech ends; a cold load alone would eat the budget.
pub fn prewarm(app: &AppHandle) {
    let Some(s) = state(app) else { return };
    let cfg = s.config();
    if !cfg.cleanup_enabled {
        return;
    }
    let client = s.client();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = client.preload(&cfg.model, &cfg.keep_alive).await {
            log::debug!("L.O.B.A prewarm skipped: {e}");
        }
    });
}

#[derive(Debug, Clone)]
pub struct LobaResult {
    pub text: String,
    pub outcome: Outcome,
    pub elapsed_ms: u64,
}

/// Run the pipeline on a raw transcript. `None` means L.O.B.A is not in play
/// (disabled or uninitialised) and Handy's own path should run instead.
pub async fn process(app: &AppHandle, raw: &str) -> Option<LobaResult> {
    let s = state(app)?;
    let cfg = s.config();
    if !cfg.cleanup_enabled {
        return None;
    }
    let client = s.client();
    let out = cleanup::clean(
        CleanupJob {
            client: &client,
            model: &cfg.model,
            keep_alive: &cfg.keep_alive,
            budget: Duration::from_millis(cfg.timeout_ms),
            context: PromptContext::default(),
        },
        raw,
    )
    .await;
    log::info!(
        "L.O.B.A cleanup: {:?} in {} ms",
        out.outcome,
        out.elapsed_ms
    );
    let result = LobaResult {
        text: out.text,
        outcome: out.outcome,
        elapsed_ms: out.elapsed_ms,
    };
    announce(app, &result);
    Some(result)
}

#[derive(Clone, serde::Serialize)]
struct RanEvent<'a> {
    outcome: &'a Outcome,
    elapsed_ms: u64,
}

/// Tell the window how the last dictation went, so a silent fallback is
/// never invisible.
fn announce(app: &AppHandle, result: &LobaResult) {
    let _ = app.emit(
        "loba:ran",
        RanEvent {
            outcome: &result.outcome,
            elapsed_ms: result.elapsed_ms,
        },
    );
}
