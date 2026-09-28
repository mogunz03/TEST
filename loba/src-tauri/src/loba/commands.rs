//! Frontend surface for L.O.B.A.

use super::LobaState;
use loba_core::LobaConfig;
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
