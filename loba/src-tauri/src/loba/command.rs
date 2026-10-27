//! Command mode: hold the second hotkey, say what to do with the selection
//! ("make this shorter", "translate to French"), release. The selection is
//! replaced with the model's answer — or left alone if anything fails.
//!
//! Hold-to-talk: the shortcut handler calls `start` on press and `stop` on
//! release for non-transcribe bindings, so no coordinator changes are needed.

use super::{state, LobaState};
use crate::actions::ShortcutAction;
use crate::audio_feedback::{play_feedback_sound, SoundType};
use crate::audio_toolkit::VadPolicy;
use crate::input::EnigoState;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::get_settings;
use crate::tray::{set_tray_state, TrayIconState};
use crate::utils;
use enigo::{Direction, Key, Keyboard};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub const BINDING_ID: &str = "loba_command";

/// idle → recording (press) → processing (release) → idle. Presses and
/// releases that don't fit the current phase are ignored, so a second press
/// while the model is still thinking can't tear down the first run.
static PHASE: AtomicU8 = AtomicU8::new(IDLE);
const IDLE: u8 = 0;
const RECORDING: u8 = 1;
const PROCESSING: u8 = 2;
/// Cancel generation captured at press, checked at every step after release.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub struct CommandAction;

fn finish(app: &AppHandle) {
    utils::hide_recording_overlay(app);
    set_tray_state(app, TrayIconState::Idle);
    PHASE.store(IDLE, Ordering::SeqCst);
}

fn fail(app: &AppHandle, why: String) {
    log::warn!("L.O.B.A command: {why}");
    let _ = app.emit("loba:command-failed", why);
    finish(app);
}

impl ShortcutAction for CommandAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        if PHASE
            .compare_exchange(IDLE, RECORDING, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        let rm = app.state::<Arc<AudioRecordingManager>>();
        let tm = app.state::<Arc<TranscriptionManager>>();

        tm.initiate_model_load();
        if let Some(s) = state(app) {
            super::prewarm(&s);
        }

        let vad = if get_settings(app).vad_enabled {
            VadPolicy::Offline
        } else {
            VadPolicy::Disabled
        };
        GENERATION.store(rm.cancel_generation(), Ordering::SeqCst);
        match rm.try_start_recording(binding_id, vad) {
            Ok(_readiness) => {
                set_tray_state(app, TrayIconState::Recording);
                utils::show_recording_overlay(app);
                super::pause_media(app);
                play_feedback_sound(app, SoundType::Start);
                crate::shortcut::register_cancel_shortcut(app);
            }
            Err(e) => {
                // Usually "Already recording": a dictation owns the mic.
                log::debug!("L.O.B.A command not started: {e}");
                PHASE.store(IDLE, Ordering::SeqCst);
            }
        }
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        if PHASE
            .compare_exchange(RECORDING, PROCESSING, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        if !rm.is_recording() {
            // Cancelled (Esc) while held; the cancel path already cleaned up.
            PHASE.store(IDLE, Ordering::SeqCst);
            return;
        }
        crate::shortcut::unregister_cancel_shortcut(app);
        rm.remove_mute();
        super::resume_media();
        play_feedback_sound(app, SoundType::Stop);

        let app = app.clone();
        let binding_id = binding_id.to_string();
        tauri::async_runtime::spawn(async move {
            let generation = GENERATION.load(Ordering::SeqCst);
            let Some(samples) = rm.stop_recording(&binding_id, generation) else {
                return finish(&app);
            };
            if rm.was_cancelled_since(generation) || super::too_short(&samples) {
                return finish(&app);
            }
            utils::show_transcribing_overlay(&app);
            set_tray_state(&app, TrayIconState::Transcribing);

            // Grab the selection now: the hotkey is up, focus is still in the
            // target app, and the selection is still live.
            let app_for_copy = app.clone();
            let selection =
                tauri::async_runtime::spawn_blocking(move || copy_selection(&app_for_copy))
                    .await
                    .unwrap_or(None);

            let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
            let instruction =
                match tauri::async_runtime::spawn_blocking(move || tm.transcribe(samples)).await {
                    Ok(Ok(text)) => text,
                    Ok(Err(e)) => return fail(&app, format!("transcription failed: {e}")),
                    Err(e) => return fail(&app, format!("transcription panicked: {e}")),
                };
            if rm.was_cancelled_since(generation) {
                return finish(&app);
            }

            let Some(s) = state(&app) else {
                return fail(&app, "L.O.B.A not initialised".into());
            };
            utils::show_processing_overlay(&app);
            let result = run_llm(&s, &instruction, selection.as_deref()).await;
            if rm.was_cancelled_since(generation) {
                return finish(&app);
            }

            match result {
                Ok(text) => {
                    let ah = app.clone();
                    let queued = app.run_on_main_thread(move || {
                        let pasted = match utils::paste(text, ah.clone()) {
                            Ok(()) => true,
                            Err(e) => {
                                log::error!("L.O.B.A command paste failed: {e}");
                                let _ = ah.emit("paste-error", ());
                                false
                            }
                        };
                        finish(&ah);
                        if pasted {
                            crate::loba::on_pasted(&ah);
                        }
                    });
                    // If the closure never runs, nothing else resets the phase.
                    if let Err(e) = queued {
                        fail(&app, format!("could not reach the main thread: {e}"));
                    }
                }
                Err(why) => fail(&app, why),
            }
        });
    }
}

async fn run_llm(
    s: &LobaState,
    instruction: &str,
    selection: Option<&str>,
) -> Result<String, String> {
    let config = s.config();
    let dictionary = s.dictionary();
    let client = s.client();
    let r = loba_core::command::run(&client, &config, &dictionary, instruction, selection).await;
    log::info!(
        "L.O.B.A command ({} chars selected) {} in {} ms",
        selection.map(|t| t.chars().count()).unwrap_or(0),
        if r.text.is_ok() { "ok" } else { "failed" },
        r.elapsed_ms
    );
    r.text
}

/// Copy the current selection via a synthetic ⌘C and give the user their
/// clipboard back. `None` when nothing was selected (the pasteboard didn't
/// change) — the command then writes fresh text at the cursor.
fn copy_selection(app: &AppHandle) -> Option<String> {
    if !loba_core::modifiers::wait_for_release(Duration::from_millis(600)) {
        log::warn!("Modifiers still held; not copying the selection");
        return None;
    }

    let clipboard = app.clipboard();
    let previous = clipboard.read_text().ok();
    // A marker we can recognise, so "unchanged" is unambiguous even when the
    // previous clipboard held the same text as the selection.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let marker = format!("\u{2063}loba-{nonce}");
    let _ = clipboard.write_text(marker.clone());

    let sent = app
        .try_state::<EnigoState>()
        .and_then(|st| st.0.lock().ok().map(|mut e| send_copy(&mut e)))
        .unwrap_or(Err("keyboard not initialised".into()));

    let mut selection = None;
    if let Err(e) = sent {
        log::warn!("Could not send copy: {e}");
    } else {
        let deadline = Instant::now() + Duration::from_millis(400);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            match clipboard.read_text() {
                Ok(t) if t != marker => {
                    selection = Some(t);
                    break;
                }
                _ => {}
            }
        }
    }

    // Restore before anything else touches the pasteboard: Handy's paste
    // saves and restores "the current clipboard", which must be the user's.
    match previous {
        Some(p) => {
            let _ = clipboard.write_text(p);
        }
        None => {
            let _ = clipboard.clear();
        }
    }
    selection.filter(|s| !s.trim().is_empty())
}

fn send_copy(enigo: &mut enigo::Enigo) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let (modifier, c) = (Key::Meta, Key::Other(0x08)); // kVK_ANSI_C
    #[cfg(not(target_os = "macos"))]
    let (modifier, c) = (Key::Control, Key::Unicode('c'));

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| e.to_string())?;
    let click = enigo.key(c, Direction::Click).map_err(|e| e.to_string());
    std::thread::sleep(Duration::from_millis(15));
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| e.to_string())?;
    click
}
