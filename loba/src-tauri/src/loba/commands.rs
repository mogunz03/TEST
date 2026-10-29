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
pub struct StatsView {
    pub dictations: u32,
    pub words: u32,
    pub days: u32,
    /// Consecutive days up to today (or yesterday, so a streak doesn't read
    /// as broken before the first dictation of the morning).
    pub streak: u32,
    pub minutes_saved: f64,
}

/// Running totals for Home.
#[tauri::command]
#[specta::specta]
pub fn loba_get_stats(state: State<'_, Arc<LobaState>>) -> StatsView {
    let s = state.stats();
    let today = chrono::Local::now().date_naive();
    StatsView {
        dictations: s.dictations.min(u32::MAX as u64) as u32,
        words: s.words.min(u32::MAX as u64) as u32,
        days: s.days.len() as u32,
        streak: streak(&s.days, today),
        minutes_saved: s.minutes_saved(),
    }
}

fn streak(days: &[String], today: chrono::NaiveDate) -> u32 {
    let parsed: Vec<chrono::NaiveDate> = days
        .iter()
        .filter_map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .collect();
    let Some(&last) = parsed.last() else { return 0 };
    if (today - last).num_days() > 1 {
        return 0;
    }
    let mut n = 1;
    for w in parsed.windows(2).rev() {
        if (w[1] - w[0]).num_days() == 1 {
            n += 1;
        } else {
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::streak;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn streak_counts_back_from_today_or_yesterday() {
        let days: Vec<String> = ["2026-09-20", "2026-09-27", "2026-09-28"]
            .map(String::from)
            .to_vec();
        assert_eq!(streak(&days, d("2026-09-28")), 2);
        assert_eq!(streak(&days, d("2026-09-29")), 2);
        assert_eq!(streak(&days, d("2026-09-30")), 0);
        assert_eq!(streak(&[], d("2026-09-30")), 0);
    }
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

/// Transcribe a past dictation again in `language`, clean it up for the app
/// it was made in, save it over the old text and put it on the clipboard.
/// For when Whisper picked the wrong language and the words came out wrong.
#[tauri::command]
#[specta::specta]
pub async fn loba_redo_dictation(
    app: tauri::AppHandle,
    state: State<'_, Arc<LobaState>>,
    history: State<'_, Arc<crate::managers::history::HistoryManager>>,
    transcription: State<'_, Arc<crate::managers::transcription::TranscriptionManager>>,
    id: i64,
    language: String,
) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    let entry = history
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or("That dictation is gone from history")?;
    let samples =
        crate::audio_toolkit::read_wav_samples(history.get_audio_file_path(&entry.file_name))
            .map_err(|_| "The recording for this dictation was not kept".to_string())?;
    if samples.is_empty() {
        return Err("The recording for this dictation is empty".into());
    }

    transcription.initiate_model_load();
    let tm = Arc::clone(&transcription);
    let raw =
        tauri::async_runtime::spawn_blocking(move || tm.transcribe_as(samples, Some(language)))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    if raw.trim().is_empty() {
        return Err("No speech found in the recording".into());
    }

    let row_app = history
        .loba_recent(500)
        .ok()
        .and_then(|rows| rows.into_iter().find(|r| r.id == id))
        .and_then(|r| r.app);
    state.set_target(AppContext {
        name: row_app,
        ..Default::default()
    });

    let processed =
        crate::actions::process_transcription_output(&app, &raw, entry.post_process_requested)
            .await;
    let text = processed.final_text.clone();
    history
        .update_transcription(
            id,
            raw,
            processed.post_processed_text,
            processed.post_process_prompt,
        )
        .map_err(|e| e.to_string())?;
    if let Some(loba) = &processed.loba {
        super::record_history(&app, &history, id, loba);
    }
    app.clipboard()
        .write_text(text.clone())
        .map_err(|e| e.to_string())?;
    Ok(text)
}

// --- meetings ----------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub fn loba_voice_model_status(app: tauri::AppHandle) -> super::voices::VoiceModelStatus {
    super::voices::status(&app)
}

/// Fetch the voice model (26 MB). Only ever runs when the user asks.
#[tauri::command]
#[specta::specta]
pub async fn loba_voice_model_download(app: tauri::AppHandle) -> Result<(), String> {
    super::voices::download(&app).await
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting_rename_voice(
    app: tauri::AppHandle,
    id: String,
    voice: u32,
    name: String,
) -> Result<loba_core::meeting::Meeting, String> {
    super::meeting::rename_voice(&app, &id, voice, &name)
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting_start(app: tauri::AppHandle) -> Result<super::meeting::MeetingStatus, String> {
    super::meeting::start(&app)
}

/// Stops recording and returns the meeting id; the recap follows as
/// `loba:meetings-changed`.
#[tauri::command]
#[specta::specta]
pub fn loba_meeting_stop(app: tauri::AppHandle) -> Result<String, String> {
    super::meeting::stop(&app)
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting_status() -> super::meeting::MeetingStatus {
    super::meeting::status()
}

#[tauri::command]
#[specta::specta]
pub fn loba_meetings(app: tauri::AppHandle) -> Vec<loba_core::meeting::MeetingSummary> {
    super::meeting::list(&app)
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting(
    app: tauri::AppHandle,
    id: String,
) -> Result<loba_core::meeting::Meeting, String> {
    super::meeting::get(&app, &id)
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    super::meeting::delete(&app, &id)
}

#[tauri::command]
#[specta::specta]
pub fn loba_meeting_recap(app: tauri::AppHandle, id: String) -> Result<(), String> {
    super::meeting::redo_recap(&app, &id)
}

/// The meeting as Markdown (recap, then transcript), for copying out.
#[tauri::command]
#[specta::specta]
pub fn loba_meeting_markdown(
    app: tauri::AppHandle,
    id: String,
    title: String,
) -> Result<String, String> {
    Ok(super::meeting::get(&app, &id)?.markdown(&title))
}

#[tauri::command]
#[specta::specta]
pub fn loba_open_screen_recording_settings() {
    super::meeting::open_privacy_settings();
}
