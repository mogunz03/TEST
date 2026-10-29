//! Meeting mode: the microphone is "You", the Mac's output is the call.
//! Both are transcribed in chunks while the meeting runs; when it ends the
//! local model writes a recap. Logic that doesn't need Tauri lives in
//! `loba_core::meeting`, `loba_core::voices` and `loba_core::system_audio`.
//!
//! Who said what:
//! - Mic audio that is just the call coming out of the speakers is dropped,
//!   so "You" is only you (`voices::is_echo`).
//! - With the voice model installed, the call is split at pauses, each piece
//!   fingerprinted, and the pieces grouped into Speaker A, B, C. Grouping
//!   reruns as the meeting goes, and once more at the end over everything.

use super::state;
use super::voices::VoiceModel;
use crate::audio_toolkit::{list_input_devices, AudioRecorder, VadPolicy};
use crate::managers::transcription::TranscriptionManager;
use loba_core::meeting::{
    self, cut_point, has_speech, Meeting, MeetingStore, MeetingSummary, Segment, Speaker,
    SAMPLE_RATE,
};
use loba_core::system_audio::{SystemAudio, NOT_ALLOWED};
use loba_core::voices::{self, Utterance, HOP, LAG_FRAMES, LEAD_FRAMES, SAME_SPEAKER};
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

/// Loudness of a track per 10 ms, on the meeting clock, kept for the whole
/// meeting (1.4 MB an hour). The mic's echo check reads the call's.
#[derive(Default)]
struct Loudness {
    /// Meeting frame of `frames[0]`.
    first: usize,
    frames: Vec<f32>,
    /// Samples not yet making up a whole frame.
    rest: Vec<f32>,
}

impl Loudness {
    fn push(&mut self, samples: &[f32]) {
        self.rest.extend_from_slice(samples);
        let whole = self.rest.len() / HOP * HOP;
        self.frames.extend(voices::loudness(&self.rest[..whole]));
        self.rest.drain(..whole);
    }

    /// Frames `from..to` on the meeting clock, or `None` when this track
    /// wasn't recording yet. Past what has arrived is padded with silence.
    fn window(&self, from: usize, to: usize) -> Option<Vec<f32>> {
        if from < self.first || self.frames.is_empty() {
            return None;
        }
        Some(
            (from..to)
                .map(|f| self.frames.get(f - self.first).copied().unwrap_or(0.0))
                .collect(),
        )
    }
}

struct Track {
    speaker: Speaker,
    buf: Mutex<Vec<f32>>,
    /// Meeting time of the first sample in `buf`, in samples.
    at: AtomicU64,
    started: AtomicBool,
    loudness: Mutex<Loudness>,
}

impl Track {
    fn new(speaker: Speaker) -> Self {
        Self {
            speaker,
            buf: Mutex::new(Vec::new()),
            at: AtomicU64::new(0),
            started: AtomicBool::new(false),
            loudness: Mutex::new(Loudness::default()),
        }
    }

    fn push(&self, frames: &[f32], meeting_start: Instant) {
        if !self.started.swap(true, Ordering::SeqCst) {
            // Tracks start at slightly different moments; anchor each to
            // the wall clock so "You" and "Others" line up.
            let offset = meeting_start.elapsed().as_millis() as u64 * SAMPLE_RATE as u64 / 1000;
            self.at.store(offset, Ordering::SeqCst);
            self.loudness.lock().unwrap().first = offset as usize / HOP;
        }
        self.buf.lock().unwrap().extend_from_slice(frames);
        self.loudness.lock().unwrap().push(frames);
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

/// Voice fingerprints of the call so far, and which segment each belongs to.
#[derive(Default)]
struct VoiceBook {
    prints: Vec<Vec<f32>>,
    /// Current speaker of each print, from the last grouping.
    labels: Vec<u32>,
    /// (segment start, prints in it), for relabelling segments.
    segments: Vec<(u64, Vec<usize>)>,
    /// The last voice heard, for pieces too short to fingerprint.
    last: Option<u32>,
}

impl VoiceBook {
    /// Add a print and regroup. Regrouping everything each time keeps early
    /// guesses from sticking; past 200 prints it runs every tenth, which is
    /// still under a second's work.
    fn add(&mut self, print: Vec<f32>) -> usize {
        self.prints.push(print);
        let n = self.prints.len();
        if n <= 200 || n.is_multiple_of(10) {
            self.regroup();
        } else {
            // Until the next regroup, join the closest voice heard so far.
            let label = self.nearest(n - 1);
            self.labels.push(label);
        }
        n - 1
    }

    fn regroup(&mut self) {
        self.labels = voices::cluster(&self.prints, SAME_SPEAKER);
    }

    fn nearest(&self, i: usize) -> u32 {
        let dot = |a: &[f32], b: &[f32]| -> f32 {
            let n = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
            a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>() / (n(a) * n(b))
        };
        let next = self.labels.iter().max().map_or(0, |m| m + 1);
        self.labels
            .iter()
            .enumerate()
            .map(|(j, &l)| (dot(&self.prints[i], &self.prints[j]), l))
            .filter(|(s, _)| *s >= SAME_SPEAKER)
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(next, |(_, l)| l)
    }

    /// The speaker most of these prints belong to.
    fn majority(&self, prints: &[usize]) -> Option<u32> {
        let mut counts = std::collections::BTreeMap::new();
        for &p in prints {
            if let Some(&l) = self.labels.get(p) {
                *counts.entry(l).or_insert(0) += 1;
            }
        }
        counts.into_iter().max_by_key(|&(_, c)| c).map(|(l, _)| l)
    }

    /// Bring every call segment's voice in line with the latest grouping.
    fn relabel(&self, meeting: &mut Meeting) {
        for (at, prints) in &self.segments {
            let Some(voice) = self.majority(prints) else {
                continue;
            };
            if let Some(seg) = meeting
                .segments
                .iter_mut()
                .find(|s| s.speaker == Speaker::Others && s.start_ms == *at)
            {
                seg.voice = Some(voice);
            }
        }
    }
}

struct Session {
    started: Instant,
    meeting: Mutex<Meeting>,
    tracks: [Arc<Track>; 2],
    voices: Option<VoiceModel>,
    book: Mutex<VoiceBook>,
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

    let voices = system.as_ref().and_then(|_| VoiceModel::load(app));
    log::info!(
        "Meeting voices: {}",
        if voices.is_some() {
            "telling the call's speakers apart"
        } else {
            "one Others for the call (voice model not installed)"
        }
    );

    let session = Arc::new(Session {
        started,
        meeting: Mutex::new(meeting),
        tracks,
        voices,
        book: Mutex::new(VoiceBook::default()),
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

/// A stretch of a chunk to transcribe as one segment.
struct Run {
    from: usize,
    to: usize,
    voice: Option<u32>,
    prints: Vec<usize>,
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
    let pieces = voices::utterances(&chunk);
    let runs = match speaker {
        Speaker::You => your_runs(session, start_ms, &chunk, &pieces),
        Speaker::Others => call_runs(session, &chunk, &pieces),
    };
    for run in runs {
        transcribe_run(app, tm, session, speaker, start_ms, &chunk, run);
    }
}

/// Your mic, minus the stretches that are only the call coming out of the
/// speakers. Consecutive kept pieces become one run.
fn your_runs(session: &Session, start_ms: u64, chunk: &[f32], pieces: &[Utterance]) -> Vec<Run> {
    if pieces.is_empty() {
        return vec![Run {
            from: 0,
            to: chunk.len(),
            voice: None,
            prints: vec![],
        }];
    }
    let call = session.tracks[1].loudness.lock().unwrap();
    let base = start_ms as usize / 10; // meeting frame of chunk[0]
    let mut runs: Vec<Run> = Vec::new();
    let mut joined = false;
    for u in pieces {
        let (a, b) = (base + u.start / HOP, base + u.end / HOP);
        let echo = a >= LEAD_FRAMES
            && call
                .window(a - LEAD_FRAMES, b + LAG_FRAMES)
                .is_some_and(|c| {
                    let mic = voices::loudness(&chunk[u.start..u.end]);
                    let score = voices::echo_score(&mic, &c);
                    log::debug!("Meeting mic at {} ms: echo score {score:.2}", a * 10);
                    voices::is_echo(&mic, &c)
                });
        if echo {
            joined = false;
            continue;
        }
        match runs.last_mut() {
            Some(r) if joined => r.to = u.end,
            _ => runs.push(Run {
                from: u.start,
                to: u.end,
                voice: None,
                prints: vec![],
            }),
        }
        joined = true;
    }
    let dropped = pieces.len() - runs.len();
    if runs.is_empty() || dropped > 0 {
        log::info!(
            "Meeting mic at {start_ms} ms: kept {} of {} pieces, the rest was the call's echo",
            runs.len(),
            pieces.len()
        );
    }
    runs
}

/// The call, split by voice when the voice model is there.
fn call_runs(session: &Session, chunk: &[f32], pieces: &[Utterance]) -> Vec<Run> {
    let whole = || {
        vec![Run {
            from: 0,
            to: chunk.len(),
            voice: None,
            prints: vec![],
        }]
    };
    let Some(model) = session.voices.as_ref() else {
        return whole();
    };
    if pieces.is_empty() {
        return whole();
    }
    let mut book = session.book.lock().unwrap();
    // Fingerprint what can be; short pieces borrow a neighbour's voice.
    let mut tagged: Vec<(Utterance, Option<usize>)> = pieces
        .iter()
        .map(|u| {
            let print = u
                .can_fingerprint()
                .then(|| model.fingerprint(&chunk[u.start..u.end]))
                .flatten()
                .map(|p| book.add(p));
            (*u, print)
        })
        .collect();
    let mut voices: Vec<Option<u32>> = tagged
        .iter()
        .map(|(_, p)| p.and_then(|p| book.labels.get(p).copied()))
        .collect();
    for i in 0..voices.len() {
        if voices[i].is_none() {
            voices[i] = voices[..i]
                .iter()
                .rev()
                .flatten()
                .next()
                .or_else(|| voices[i + 1..].iter().flatten().next())
                .copied()
                .or(book.last);
        }
    }
    if let Some(v) = voices.iter().rev().flatten().next() {
        book.last = Some(*v);
    }

    let mut runs: Vec<Run> = Vec::new();
    for ((u, print), voice) in tagged.drain(..).zip(voices) {
        match runs.last_mut() {
            Some(r) if r.voice == voice => {
                r.to = u.end;
                r.prints.extend(print);
            }
            _ => runs.push(Run {
                from: u.start,
                to: u.end,
                voice,
                prints: print.into_iter().collect(),
            }),
        }
    }
    runs
}

/// Whisper on less than this tends to invent a word or two.
const MIN_RUN: usize = SAMPLE_RATE / 2;

fn transcribe_run(
    app: &AppHandle,
    tm: &TranscriptionManager,
    session: &Session,
    speaker: Speaker,
    chunk_ms: u64,
    chunk: &[f32],
    run: Run,
) {
    let audio = &chunk[run.from..run.to];
    if audio.len() < MIN_RUN || !has_speech(audio) {
        return;
    }
    let start_ms = chunk_ms + (run.from as u64 * 1000 / SAMPLE_RATE as u64);
    let text = match tm.transcribe(audio.to_vec()) {
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
        voice: run.voice,
    };
    {
        let mut m = session.meeting.lock().unwrap();
        let at = m.segments.partition_point(|s| s.start_ms <= start_ms);
        m.segments.insert(at, segment.clone());
        if !run.prints.is_empty() {
            let mut book = session.book.lock().unwrap();
            book.segments.push((start_ms, run.prints));
            book.relabel(&mut m);
        }
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
        {
            // One last grouping over the whole call, now that every voice
            // has been heard more than once.
            let mut book = session.book.lock().unwrap();
            if !book.prints.is_empty() {
                book.regroup();
                book.relabel(&mut meeting);
                log::info!(
                    "Meeting {}: {} voices on the call",
                    meeting.id,
                    book.labels.iter().max().map_or(0, |m| m + 1)
                );
            }
        }
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

async fn write_recap(app: &AppHandle, meeting: Meeting) {
    let Some(s) = state(app) else { return };
    let cfg = s.config();
    let result = meeting::recap(&s.client(), &cfg.model, &cfg.keep_alive, &meeting).await;
    // Speakers may have been renamed while the recap was being written:
    // start from what is saved now, and bring the recap in line with it.
    let mut meeting = store(app)
        .and_then(|st| st.load(&meeting.id).ok())
        .unwrap_or(meeting);
    match result {
        Ok(r) => meeting.set_recap(r),
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

/// Name one of the call's voices, in this meeting.
pub fn rename_voice(app: &AppHandle, id: &str, voice: u32, name: &str) -> Result<Meeting, String> {
    let st = store(app).ok_or("L.O.B.A isn't initialised")?;
    let mut meeting = st.load(id)?;
    meeting.rename_voice(voice, name);
    st.save(&meeting)?;
    let _ = app.emit("loba:meetings-changed", ());
    Ok(meeting)
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
