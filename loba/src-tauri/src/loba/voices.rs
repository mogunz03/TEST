//! The voice model that tells the people on a call apart.
//!
//! A 26 MB speaker-embedding network (WeSpeaker ResNet34, VoxCeleb), fetched
//! from the sherpa-onnx releases on GitHub only when the user asks for it,
//! checked against a fixed hash, and run locally with the ONNX runtime the
//! speech models already use. Feature extraction and grouping live in
//! `loba_core::voices`.

use super::state;
use futures_util::StreamExt;
use loba_core::voices::{fbank, MEL};
use ort::session::Session;
use ort::value::TensorRef;
use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const FILE: &str = "wespeaker_en_voxceleb_resnet34.onnx";
const URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34.onnx";
const SHA256: &str = "5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94";
pub const SIZE_BYTES: u64 = 26_534_365;

static DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize, Type)]
pub struct VoiceModelStatus {
    pub installed: bool,
    pub downloading: bool,
    pub size_bytes: u64,
}

#[derive(Clone, Serialize, Type)]
struct Progress {
    downloaded: u64,
    total: u64,
}

fn path(app: &AppHandle) -> Option<PathBuf> {
    state(app).map(|s| s.dir().join("models").join(FILE))
}

pub fn status(app: &AppHandle) -> VoiceModelStatus {
    VoiceModelStatus {
        installed: path(app).is_some_and(|p| p.exists()),
        downloading: DOWNLOADING.load(Ordering::SeqCst),
        size_bytes: SIZE_BYTES,
    }
}

/// Fetch the model. Progress goes out as `loba:voice-model-progress`; the
/// file only appears under its real name once the hash matches.
pub async fn download(app: &AppHandle) -> Result<(), String> {
    let dest = path(app).ok_or("L.O.B.A isn't initialised")?;
    if dest.exists() {
        return Ok(());
    }
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("Already downloading".into());
    }
    let result = fetch(app, &dest).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    let _ = app.emit("loba:voice-model", status(app));
    result
}

async fn fetch(app: &AppHandle, dest: &Path) -> Result<(), String> {
    let dir = dest.parent().ok_or("bad model path")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let part = dest.with_extension("onnx.part");

    let res = reqwest::get(URL)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?;
    let total = res.content_length().unwrap_or(SIZE_BYTES);
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut downloaded = 0u64;
    let mut last = Instant::now();
    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download stopped: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        hash.update(&chunk);
        downloaded += chunk.len() as u64;
        if last.elapsed() >= Duration::from_millis(150) {
            let _ = app.emit("loba:voice-model-progress", Progress { downloaded, total });
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    let got = format!("{:x}", hash.finalize());
    if got != SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err("The download didn't match the expected file".into());
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())?;
    log::info!("Voice model installed at {}", dest.display());
    Ok(())
}

/// A loaded model. One meeting at a time uses it, from its worker thread.
pub struct VoiceModel(Mutex<Session>);

impl VoiceModel {
    /// `None` when the model isn't installed or won't load; meetings then
    /// fall back to one "Others" for the whole call.
    pub fn load(app: &AppHandle) -> Option<Self> {
        let p = path(app).filter(|p| p.exists())?;
        let load = || -> Result<Session, String> {
            let e = |e: &dyn std::fmt::Display| e.to_string();
            Session::builder()
                .map_err(|x| e(&x))?
                .with_intra_threads(2)
                .map_err(|x| e(&x))?
                .commit_from_file(&p)
                .map_err(|x| e(&x))
        };
        match load() {
            Ok(s) => Some(Self(Mutex::new(s))),
            Err(e) => {
                log::warn!("Voice model didn't load: {e}");
                None
            }
        }
    }

    /// The voice fingerprint of a piece of speech (16 kHz mono).
    pub fn fingerprint(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let (frames, feats) = fbank(samples);
        if frames == 0 {
            return None;
        }
        let input = TensorRef::from_array_view(([1usize, frames, MEL], &feats[..])).ok()?;
        let mut session = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let out = session
            .run(ort::inputs!["feats" => input])
            .map_err(|e| log::warn!("Voice fingerprint failed: {e}"))
            .ok()?;
        let (_, print) = out[0].try_extract_tensor::<f32>().ok()?;
        Some(print.to_vec())
    }
}
