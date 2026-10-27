//! Meeting mode: the microphone is "You", the Mac's output is "Others".
//! Both are transcribed in chunks while the meeting runs; when it ends the
//! local model writes a recap. Logic that doesn't need Tauri lives in
//! `loba_core::meeting` and `loba_core::system_audio`.

use super::state;
use crate::audio_toolkit::{list_input_devices, AudioRecorder, VadPolicy};
use crate::managers::transcription::TranscriptionManager;
use loba_core::meeting::{
    self, cut_point, has_speech, Meeting, MeetingStore, MeetingSummary, Segment, Speaker,
    SAMPLE_RATE,
};
use loba_core::system_audio::{SystemAudio, NOT_ALLOWED};
use once_cell::sync::Lazy;
use serde::Serialize;
use specta::Type;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const PILL: &str = "meeting_pill";
const PILL_W: f64 = 248.0;
const PILL_H: f64 = 48.0;
/// The recorder keeps its own copy of everything until stopped; restarting
/// it this often keeps an hour-long meeting from holding hundreds of MB.
const RECORDER_ROTATE: Duration = Duration::from_secs(60);

#[derive(Clone, Serialize, Type, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Recording,
    Summarizing,
}

#[derive(Clone, Serialize, Type)]
pub struct MeetingStatus {
    pub phase: Phase,
    pub id: Option<String>,
    pub elapsed_ms: u64,
    pub segments: u32,
    /// False when macOS refused system audio: only "You" is being heard.
    pub system_audio: bool,
}

struct Track {
    speaker: Speaker,
    buf: Mutex<Vec<f32>>,
    /// Meeting time of the first sample in `buf`, in samples.
    at: AtomicU64,
    started: AtomicBool,
}

impl Track {
    fn new(speaker: Speaker) -> Self {
        Self {
            speaker,
            buf: Mutex::new(Vec::new()),
            at: AtomicU64::new(0),
            started: AtomicBool::new(false),
        }
    }

    fn push(&self, frames: &[f32], meeting_start: Instant) {
        if !self.started.swap(true, Ordering::SeqCst) {
            // Tracks start at slightly different moments; anchor each to
            // the wall clock so "You" and "Others" line up.
            let offset = meeting_start.elapsed().as_millis() as u64 * SAMPLE_RATE as u64 / 1000;
            self.at.store(offset, Ordering::SeqCst);
        }
        self.buf.lock().unwrap().extend_from_slice(frames);
    }

    /// Take a chunk if one is ready (or everything, when flushing).
    fn take(&self, flush: bool) -> Option<(u64, Vec<f32>)> {
        let mut buf = self.buf.lock().unwrap();
        let cut = if flush {
            (!buf.is_empty()).then_some(buf.len())
        } else {
            cut_point(&buf)
        }?;
        let chunk: Vec<f32> = buf.drain(..cut).collect();
        let start = self.at.fetch_add(cut as u64, Ordering::SeqCst);
        Some((start * 1000 / SAMPLE_RATE as u64, chunk))
    }
}

struct Session {
    started: Instant,
    meeting: Mutex<Meeting>,
    tracks: [Arc<Track>; 2],
    stop: AtomicBool,
    mic: Mutex<Option<AudioRecorder>>,
    system: Mutex<Option<SystemAudio>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

static SESSION: Lazy<Mutex<Option<Arc<Session>>>> = Lazy::new(|| Mutex::new(None));
static SUMMARIZING: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

fn store(app: &AppHandle) -> Option<MeetingStore> {
    state(app).map(|s| MeetingStore::new(s.dir().join("meetings")))
}

pub fn status() -> MeetingStatus {
    if let Some(s) = SESSION.lock().unwrap().as_ref() {
        let m = s.meeting.lock().unwrap();
        return MeetingStatus {
            phase: Phase::Recording,
            id: Some(m.id.clone()),
            elapsed_ms: s.started.elapsed().as_millis() as u64,
            segments: m.segments.len() as u32,
            system_audio: m.system_audio,
        };
    }
    let summarizing = SUMMARIZING.lock().unwrap().clone();
    MeetingStatus {
        phase: if summarizing.is_some() {
            Phase::Summarizing
        } else {
            Phase::Idle
        },
        id: summarizing,
        elapsed_ms: 0,
        segments: 0,
        system_audio: false,
    }
}

fn announce(app: &AppHandle) {
    let _ = app.emit("loba:meeting", status());
}

fn open_mic(app: &AppHandle, track: Arc<Track>, start: Instant) -> Result<AudioRecorder, String> {
    let settings = crate::settings::get_settings(app);
    let device = settings.selected_microphone.as_ref().and_then(|name| {
        list_input_devices()
            .ok()?
            .into_iter()
            .find(|d| &d.name == name)
            .map(|d| d.device)
    });
    let mut rec = AudioRecorder::new()
        .map_err(|e| e.to_string())?
        .with_audio_callback(move |frames| track.push(frames, start));
    rec.open(device).map_err(|e| e.to_string())?;
    rec.start(VadPolicy::Disabled).map_err(|e| e.to_string())?;
    Ok(rec)
}

pub fn start(app: &AppHandle) -> Result<MeetingStatus, String> {
    let mut slot = SESSION.lock().unwrap();
    if slot.is_some() {
        return Ok(drop_and(slot, status));
    }
    let now = chrono::Local::now();
    let started = Instant::now();
    let tracks = [
        Arc::new(Track::new(Speaker::You)),
        Arc::new(Track::new(Speaker::Others)),
    ];

    let mic = open_mic(app, Arc::clone(&tracks[0]), started)
        .map_err(|e| format!("Couldn't open the microphone: {e}"))?;

    let others = Arc::clone(&tracks[1]);
    let system = match SystemAudio::start(Arc::new(move |f: &[f32]| others.push(f, started))) {
        Ok(s) => Some(s),
        Err(e) => {
            if e != NOT_ALLOWED {
                log::warn!("Meeting: system audio failed: {e}");
            }
            None
        }
    };

    let meeting = Meeting {
        id: now.format("%Y%m%d-%H%M%S").to_string(),
        started_at: now.timestamp(),
        system_audio: system.is_some(),
        ..Default::default()
    };
    log::info!(
        "Meeting {} started ({})",
        meeting.id,
        if system.is_some() {
            "mic + system audio"
        } else {
            "mic only"
        }
    );

    app.state::<Arc<TranscriptionManager>>()
        .initiate_model_load();
    if let Some(s) = state(app) {
        super::prewarm(&s);
    }

    let session = Arc::new(Session {
        started,
        meeting: Mutex::new(meeting),
        tracks,
        stop: AtomicBool::new(false),
        mic: Mutex::new(Some(mic)),
        system: Mutex::new(system),
        worker: Mutex::new(None),
    });
    let worker = {
        let app = app.clone();
        let session = Arc::clone(&session);
        std::thread::spawn(move || run(app, session))
    };
    *session.worker.lock().unwrap() = Some(worker);
    *slot = Some(session);
    drop(slot);

    show_pill(app);
    announce(app);
    Ok(status())
}

fn drop_and<T, R>(guard: T, f: impl FnOnce() -> R) -> R {
    drop(guard);
    f()
}

/// Cut, transcribe, repeat, until asked to stop; then flush what's left.
fn run(app: AppHandle, session: Arc<Session>) {
    let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
    let mut last_rotate = Instant::now();
    loop {
        let stopping = session.stop.load(Ordering::SeqCst);
        let mut worked = false;
        for track in &session.tracks {
            while let Some((start_ms, chunk)) = track.take(stopping) {
                worked = true;
                transcribe(&app, &tm, &session, track.speaker, start_ms, chunk);
                if stopping {
                    break;
                }
            }
        }
        if stopping {
            return;
        }
        if last_rotate.elapsed() >= RECORDER_ROTATE {
            if let Some(rec) = session.mic.lock().unwrap().as_ref() {
                let _ = rec.stop();
                let _ = rec.start(VadPolicy::Disabled);
            }
            last_rotate = Instant::now();
        }
        if !worked {
            std::thread::sleep(Duration::from_millis(400));
        }
    }
}

fn transcribe(
    app: &AppHandle,
    tm: &TranscriptionManager,
    session: &Session,
    speaker: Speaker,
    start_ms: u64,
    chunk: Vec<f32>,
) {
    if !has_speech(&chunk) {
        return;
    }
    let text = match tm.transcribe(chunk) {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            log::warn!("Meeting chunk at {start_ms} ms failed: {e}");
            return;
        }
    };
    if text.is_empty() {
        return;
    }
    let text = state(app)
        .map(|s| s.dictionary().apply(&text))
        .unwrap_or(text);
    let segment = Segment {
        start_ms,
        speaker,
        text,
    };
    {
        let mut m = session.meeting.lock().unwrap();
        let at = m.segments.partition_point(|s| s.start_ms <= start_ms);
        m.segments.insert(at, segment.clone());
    }
    let _ = app.emit("loba:meeting-segment", segment);
    announce(app);
}

/// Stop recording, finish transcribing, save, then write the recap in the
/// background. Returns the meeting id right away.
pub fn stop(app: &AppHandle) -> Result<String, String> {
    let session = SESSION
        .lock()
        .unwrap()
        .take()
        .ok_or("No meeting is being recorded")?;
    session.stop.store(true, Ordering::SeqCst);
    if let Some(mut rec) = session.mic.lock().unwrap().take() {
        let _ = rec.stop();
        let _ = rec.close();
    }
    if let Some(sys) = session.system.lock().unwrap().take() {
        sys.stop();
    }
    hide_pill(app);
    let id = session.meeting.lock().unwrap().id.clone();
    *SUMMARIZING.lock().unwrap() = Some(id.clone());
    announce(app);

    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(w) = session.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        let mut meeting = session.meeting.lock().unwrap().clone();
        meeting.duration_ms = session.started.elapsed().as_millis() as u64;
        if let Some(st) = store(&app) {
            if let Err(e) = st.save(&meeting) {
                log::error!("Couldn't save meeting {}: {e}", meeting.id);
            }
        }
        let _ = app.emit("loba:meetings-changed", ());
        tauri::async_runtime::block_on(write_recap(&app, meeting));
        *SUMMARIZING.lock().unwrap() = None;
        announce(&app);
    });
    Ok(id)
}

async fn write_recap(app: &AppHandle, mut meeting: Meeting) {
    let Some(s) = state(app) else { return };
    let cfg = s.config();
    match meeting::recap(&s.client(), &cfg.model, &cfg.keep_alive, &meeting).await {
        Ok(r) => {
            meeting.recap = Some(r);
            meeting.recap_error = None;
        }
        Err(e) => {
            log::warn!("Recap of {} failed: {e}", meeting.id);
            meeting.recap_error = Some(e);
        }
    }
    if let Some(st) = store(app) {
        if let Err(e) = st.save(&meeting) {
            log::error!("Couldn't save recap for {}: {e}", meeting.id);
        }
    }
    let _ = app.emit("loba:meetings-changed", ());
}

/// Write the recap again (after fixing Ollama, or with another model).
pub fn redo_recap(app: &AppHandle, id: &str) -> Result<(), String> {
    let st = store(app).ok_or("L.O.B.A isn't initialised")?;
    let meeting = st.load(id)?;
    *SUMMARIZING.lock().unwrap() = Some(id.to_string());
    announce(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        write_recap(&app, meeting).await;
        *SUMMARIZING.lock().unwrap() = None;
        announce(&app);
    });
    Ok(())
}

pub fn list(app: &AppHandle) -> Vec<MeetingSummary> {
    store(app).map(|s| s.list()).unwrap_or_default()
}

pub fn get(app: &AppHandle, id: &str) -> Result<Meeting, String> {
    store(app).ok_or("L.O.B.A isn't initialised")?.load(id)
}

pub fn delete(app: &AppHandle, id: &str) -> Result<(), String> {
    store(app).ok_or("L.O.B.A isn't initialised")?.delete(id)?;
    let _ = app.emit("loba:meetings-changed", ());
    Ok(())
}

// --- the floating pill ------------------------------------------------------

fn show_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.show();
        return;
    }
    // Top-right corner of the main display, clear of the menu bar.
    let (x, y) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let scale = m.scale_factor();
            let w = m.size().width as f64 / scale;
            let px = m.position().x as f64 / scale;
            let py = m.position().y as f64 / scale;
            (px + w - PILL_W - 16.0, py + 44.0)
        })
        .unwrap_or((40.0, 44.0));
    let mut builder =
        WebviewWindowBuilder::new(app, PILL, WebviewUrl::App("src/meeting/index.html".into()))
            .title("Meeting")
            .inner_size(PILL_W, PILL_H)
            .position(x, y)
            .resizable(false)
            .maximizable(false)
            .minimizable(false)
            .closable(false)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .accept_first_mouse(true)
            .focused(false);
    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }
    if let Err(e) = builder.build() {
        log::warn!("Couldn't open the meeting pill: {e}");
    }
}

fn hide_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.close();
    }
}

/// System Settings → Privacy & Security → Screen & System Audio Recording.
pub fn open_privacy_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
            .spawn();
    }
}
