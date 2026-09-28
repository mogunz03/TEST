//! Frontend surface for L.O.B.A.

use super::LobaState;
use loba_core::styles::Resolved;
use loba_core::{AppContext, Dictionary, LobaConfig, Styles};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
#[specta::specta]
pub fn loba_get_config(state: State<'_, Arc<LobaState>>) -> LobaConfig {
    state.config()
}

#[tauri::command]
#[specta::specta]
pub fn loba_set_config(
    state: State<'_, Arc<LobaState>>,
    config: LobaConfig,
) -> Result<LobaConfig, String> {
    state.set_config(config)
}

/// Installed Ollama models. `Err` doubles as the "Ollama not running" signal.
#[tauri::command]
#[specta::specta]
pub async fn loba_list_models(state: State<'_, Arc<LobaState>>) -> Result<Vec<String>, String> {
    state
        .client()
        .list_models()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn loba_data_dir(state: State<'_, Arc<LobaState>>) -> String {
    state.dir().display().to_string()
}

#[tauri::command]
#[specta::specta]
pub fn loba_get_dictionary(state: State<'_, Arc<LobaState>>) -> Dictionary {
    state.dictionary()
}

/// Saves `dictionary.json` and refreshes the Whisper prompt immediately.
#[tauri::command]
#[specta::specta]
pub fn loba_set_dictionary(
    state: State<'_, Arc<LobaState>>,
    dictionary: Dictionary,
) -> Result<Dictionary, String> {
    state.set_dictionary(dictionary)
}

#[tauri::command]
#[specta::specta]
pub fn loba_get_styles(state: State<'_, Arc<LobaState>>) -> Styles {
    state.styles()
}

#[tauri::command]
#[specta::specta]
pub fn loba_set_styles(state: State<'_, Arc<LobaState>>, styles: Styles) -> Result<Styles, String> {
    state.set_styles(styles)
}

#[derive(serde::Serialize, specta::Type)]
pub struct Probe {
    pub app: AppContext,
    pub style: Resolved,
}

/// What's frontmost after `delay_ms`, and the style it resolves to — lets the
/// Styles page check a mapping once the user has switched to that app.
#[tauri::command]
#[specta::specta]
pub async fn loba_probe_frontmost(
    state: State<'_, Arc<LobaState>>,
    delay_ms: u32,
) -> Result<Probe, String> {
    tokio::time::sleep(std::time::Duration::from_millis(delay_ms.min(10_000) as u64)).await;
    let app = tauri::async_runtime::spawn_blocking(loba_core::frontmost::frontmost)
        .await
        .unwrap_or_default();
    let style = state.styles().resolve(&app);
    Ok(Probe { app, style })
}

/// The history panel: newest dictations first, raw next to cleaned.
#[tauri::command]
#[specta::specta]
pub fn loba_history(
    history: State<'_, Arc<crate::managers::history::HistoryManager>>,
    limit: Option<u32>,
) -> Result<Vec<crate::managers::history::LobaHistoryRow>, String> {
    history
        .loba_recent(limit.unwrap_or(50).min(500) as usize)
        .map_err(|e| e.to_string())
}

/// One-click copy from the history panel, via the native pasteboard (the
/// webview has no clipboard capability, and doesn't need one).
#[tauri::command]
#[specta::specta]
pub fn loba_copy(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}
