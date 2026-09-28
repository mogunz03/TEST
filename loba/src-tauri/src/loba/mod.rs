//! Tauri wiring for the L.O.B.A pipeline (logic lives in `loba-core`).
//!
//! Seam: `actions::process_transcription_output` calls [`process`] between
//! transcription and paste. Hotkey, capture and injection code is untouched.

pub mod commands;
mod watched;

use loba_core::cleanup::Outcome;
use loba_core::pipeline::{self, Pipeline};
use loba_core::{Dictionary, LobaConfig, OllamaClient};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Emitter, Manager};
use watched::Watched;

pub struct LobaState {
    dir: PathBuf,
    config: Watched<LobaConfig>,
    dictionary: Watched<Dictionary>,
    client: Mutex<OllamaClient>,
}

impl LobaState {
    pub fn config(&self) -> LobaConfig {
        self.config.get()
    }

    pub fn set_config(&self, next: LobaConfig) -> Result<LobaConfig, String> {
        self.config.set(next)
    }

    pub fn dictionary(&self) -> Dictionary {
        let d = self.dictionary.get();
        publish_whisper_prompt(&d);
        d
    }

    pub fn set_dictionary(&self, next: Dictionary) -> Result<Dictionary, String> {
        let d = self.dictionary.set(next)?;
        publish_whisper_prompt(&d);
        Ok(d)
    }

    /// Client for the configured URL; rebuilt only when the URL changes.
    pub fn client(&self) -> OllamaClient {
        let url = self.config().ollama_url;
        let mut c = self.client.lock().unwrap_or_else(|p| p.into_inner());
        if c.base_url() != url {
            *c = OllamaClient::new(&url);
        }
        c.clone()
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
    let config = Watched::open(dir.join("config.json"), LobaConfig::default, |c| {
        c.sanitized()
    });
    let dictionary = Watched::open(dir.join("dictionary.json"), Dictionary::seed, |d| {
        d.normalized()
    });
    let cfg = config.get();
    log::info!(
        "L.O.B.A cleanup {} (model {}, budget {} ms, data {})",
        if cfg.cleanup_enabled { "on" } else { "off" },
        cfg.model,
        cfg.timeout_ms,
        dir.display()
    );
    let state = Arc::new(LobaState {
        dir,
        client: Mutex::new(OllamaClient::new(&cfg.ollama_url)),
        config,
        dictionary,
    });
    publish_whisper_prompt(&state.dictionary.get());
    app.manage(state);
    prewarm(app);
}

fn state(app: &AppHandle) -> Option<Arc<LobaState>> {
    app.try_state::<Arc<LobaState>>().map(|s| Arc::clone(&s))
}

// The transcription manager reads the prompt synchronously on its own thread;
// a plain global keeps it free of any dependency on Tauri state.
static WHISPER_PROMPT: Lazy<RwLock<Option<String>>> = Lazy::new(|| RwLock::new(None));

fn publish_whisper_prompt(d: &Dictionary) {
    if let Ok(mut p) = WHISPER_PROMPT.write() {
        *p = d.whisper_prompt();
    }
}

/// Initial prompt for whisper-family models: Handy's custom words (if any)
/// followed by the L.O.B.A dictionary terms.
pub fn whisper_initial_prompt(custom_words: &[String]) -> Option<String> {
    let loba = WHISPER_PROMPT.read().ok().and_then(|p| p.clone());
    let handy = (!custom_words.is_empty()).then(|| custom_words.join(", "));
    match (handy, loba) {
        (Some(h), Some(l)) => Some(format!("{h}, {l}")),
        (h, l) => h.or(l),
    }
}

/// True when dictations will go through the cleanup pass (drives the
/// "processing" overlay state and whether Handy's own post-processing runs).
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

/// Run the pipeline on a raw transcript. `None` only if L.O.B.A failed to
/// initialise, in which case Handy's own path runs untouched.
pub async fn process(app: &AppHandle, raw: &str) -> Option<LobaResult> {
    let s = state(app)?;
    let config = s.config();
    let dictionary = s.dictionary();
    let client = s.client();
    let out = pipeline::run(
        Pipeline {
            client: &client,
            config: &config,
            dictionary: &dictionary,
        },
        raw,
    )
    .await;
    log::info!("L.O.B.A pass: {:?} in {} ms", out.outcome, out.elapsed_ms);
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
