//! Who is speaking: your voice versus the call's echo, and the call's
//! voices apart from each other.
//!
//! Everything here is plain signal work, so it is testable without a model.
//! The voice model itself (an ONNX speaker-embedding network) runs in the
//! app; it takes `fbank` features from here and hands back a fingerprint.
//!
//! - `utterances` splits a stretch of audio at pauses.
//! - `is_echo` spots the mic hearing the call through the speakers: the
//!   mic's loudness then follows the call's, a few milliseconds late.
//! - `cluster` groups fingerprints into speakers.

use std::sync::OnceLock;

pub const RATE: usize = 16_000;
/// 10 ms: the step for features, loudness and pauses.
pub const HOP: usize = 160;
const WIN: usize = 400;
const NFFT: usize = 512;
pub const MEL: usize = 80;

/// Pieces shorter than this give an unreliable fingerprint.
pub const MIN_VOICE: usize = RATE;
/// A quiet gap at least this long ends an utterance.
const PAUSE_FRAMES: usize = 35;
/// Blips shorter than this are clicks and breaths, not speech.
const MIN_FRAMES: usize = 20;
/// Short pieces join a neighbour this close, so they can be fingerprinted.
const JOIN_FRAMES: usize = 100;

/// Average-linkage similarity above which two groups are one speaker.
/// Chosen on a four-speaker recording, where 0.30 to 0.40 all found four.
pub const SAME_SPEAKER: f32 = 0.36;

// --- features ----------------------------------------------------------------

struct Tables {
    window: Vec<f64>,
    /// Per mel band: first FFT bin and its weights.
    banks: Vec<(usize, Vec<f64>)>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let window = (0..WIN)
            .map(|i| {
                let hann =
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (WIN - 1) as f64).cos();
                hann.powf(0.85) // Povey window, as Kaldi
            })
            .collect();
        let mel = |f: f64| 1127.0 * (1.0 + f / 700.0).ln();
        let (lo, hi) = (mel(20.0), mel(RATE as f64 / 2.0));
        let step = (hi - lo) / (MEL + 1) as f64;
        let banks = (0..MEL)
            .map(|m| {
                let (l, c, r) = (
                    lo + m as f64 * step,
                    lo + (m + 1) as f64 * step,
                    lo + (m + 2) as f64 * step,
                );
                let mut first = None;
                let mut w = Vec::new();
                for k in 0..NFFT / 2 {
                    let v = mel(k as f64 * RATE as f64 / NFFT as f64);
                    let weight = if v > l && v < r {
                        if v <= c {
                            (v - l) / (c - l)
                        } else {
                            (r - v) / (r - c)
                        }
                    } else {
                        0.0
                    };
                    if weight > 0.0 {
                        first.get_or_insert(k);
                        w.push(weight);
                    } else if first.is_some() {
                        break;
                    }
                }
                (first.unwrap_or(0), w)
            })
            .collect();
        Tables { window, banks }
    })
}

/// In-place radix-2 FFT over (re, im) pairs; `buf.len()` is a power of two.
fn fft(buf: &mut [(f64, f64)]) {
    let n = buf.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            buf.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0, 0.0);
            for k in 0..len / 2 {
                let (ar, ai) = buf[start + k];
                let (br, bi) = buf[start + k + len / 2];
                let (tr, ti) = (br * cr - bi * ci, br * ci + bi * cr);
                buf[start + k] = (ar + tr, ai + ti);
                buf[start + k + len / 2] = (ar - tr, ai - ti);
                let next = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = next;
            }
        }
        len <<= 1;
    }
}

/// Kaldi-style 80-band log filterbank (25 ms window, 10 ms step), with the
/// per-band mean removed, as the voice model was trained on. Samples are
/// 16 kHz in [-1, 1]. Returns `frames * MEL` values, row by row.
pub fn fbank(samples: &[f32]) -> (usize, Vec<f32>) {
    if samples.len() < WIN {
        return (0, Vec::new());
    }
    let t = tables();
    let frames = 1 + (samples.len() - WIN) / HOP;
    let mut out = vec![0f32; frames * MEL];
    let mut buf = vec![(0.0, 0.0); NFFT];
    let mut frame = [0f64; WIN];
    for f in 0..frames {
        let src = &samples[f * HOP..f * HOP + WIN];
        let mean = src.iter().map(|&s| s as f64 * 32768.0).sum::<f64>() / WIN as f64;
        for (d, &s) in frame.iter_mut().zip(src) {
            *d = s as f64 * 32768.0 - mean;
        }
        for i in (1..WIN).rev() {
            frame[i] -= 0.97 * frame[i - 1];
        }
        frame[0] -= 0.97 * frame[0];
        for (i, b) in buf.iter_mut().enumerate() {
            *b = if i < WIN {
                (frame[i] * t.window[i], 0.0)
            } else {
                (0.0, 0.0)
            };
        }
        fft(&mut buf);
        let row = &mut out[f * MEL..(f + 1) * MEL];
        for (m, (first, w)) in t.banks.iter().enumerate() {
            let e: f64 = w
                .iter()
                .enumerate()
                .map(|(i, wt)| {
                    let (re, im) = buf[first + i];
                    wt * (re * re + im * im)
                })
                .sum();
            row[m] = (e.max(f32::EPSILON as f64)).ln() as f32;
        }
    }
    for m in 0..MEL {
        let mean = (0..frames).map(|f| out[f * MEL + m]).sum::<f32>() / frames as f32;
        for f in 0..frames {
            out[f * MEL + m] -= mean;
        }
    }
    (frames, out)
}

// --- loudness and pauses -------------------------------------------------------

/// Loudness per 10 ms (RMS), the basis for pauses and the echo check.
pub fn loudness(samples: &[f32]) -> Vec<f32> {
    samples
        .chunks_exact(HOP)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / HOP as f32).sqrt())
        .collect()
}

/// Above this a 10 ms frame counts as sound: four times the quiet floor of
/// the stretch, and never below a fixed minimum so near-silence stays silent.
fn speech_level(env: &[f32]) -> f32 {
    if env.is_empty() {
        return f32::INFINITY;
    }
    let mut sorted = env.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    (sorted[sorted.len() / 10] * 4.0).max(0.004)
}

/// One stretch of talking, in samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utterance {
    pub start: usize,
    pub end: usize,
    /// 10 ms frames that actually had sound.
    pub voiced: usize,
}

impl Utterance {
    /// Enough sound for a trustworthy fingerprint: at least a second long,
    /// with half a second of it above the quiet floor.
    pub fn can_fingerprint(&self) -> bool {
        self.end - self.start >= MIN_VOICE && self.voiced >= 50
    }
}

/// Where someone is talking, split at pauses. Pieces too short to
/// fingerprint are joined to a close neighbour when there is one.
pub fn utterances(samples: &[f32]) -> Vec<Utterance> {
    let env = loudness(samples);
    let level = speech_level(&env);
    // (first frame, end frame, frames that actually had sound)
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    let mut start = None;
    let mut voiced = 0;
    let mut quiet = 0;
    for (i, &e) in env.iter().enumerate() {
        if e > level {
            start.get_or_insert(i);
            voiced += 1;
            quiet = 0;
        } else if let Some(s) = start {
            quiet += 1;
            if quiet >= PAUSE_FRAMES {
                runs.push((s, i + 1 - quiet, voiced));
                start = None;
                voiced = 0;
                quiet = 0;
            }
        }
    }
    if let Some(s) = start {
        runs.push((s, env.len() - quiet, voiced));
    }
    runs.retain(|r| r.2 >= MIN_FRAMES);

    // Join short pieces to the nearer neighbour, if it is close enough.
    let min = MIN_VOICE / HOP;
    let mut i = 0;
    while i < runs.len() {
        let (a, b, v) = runs[i];
        if b - a >= min || runs.len() == 1 {
            i += 1;
            continue;
        }
        let gap_prev = (i > 0).then(|| a - runs[i - 1].1);
        let gap_next = runs.get(i + 1).map(|n| n.0 - b);
        match (gap_prev, gap_next) {
            (Some(p), n) if p <= JOIN_FRAMES && n.is_none_or(|n| p <= n) => {
                runs[i - 1].1 = b;
                runs[i - 1].2 += v;
                runs.remove(i);
            }
            (_, Some(n)) if n <= JOIN_FRAMES => {
                runs[i + 1].0 = a;
                runs[i + 1].2 += v;
                runs.remove(i);
            }
            _ => i += 1,
        }
    }
    runs.into_iter()
        .map(|(a, b, voiced)| Utterance {
            start: a * HOP,
            end: (b * HOP).min(samples.len()),
            voiced,
        })
        .collect()
}

// --- echo ----------------------------------------------------------------------

/// How far the call may lead the mic (sound through the air, plus the two
/// capture paths starting at slightly different moments), and lag it.
pub const LEAD_FRAMES: usize = 30;
pub const LAG_FRAMES: usize = 15;
/// Correlation of loudness above which the mic is hearing the call.
const ECHO: f32 = 0.6;

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 20 {
        return 0.0;
    }
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f32>() / n as f32;
    let mb = b.iter().sum::<f32>() / n as f32;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        cov += (x - ma) * (y - mb);
        va += (x - ma) * (x - ma);
        vb += (y - mb) * (y - mb);
    }
    if va <= f32::EPSILON || vb <= f32::EPSILON {
        return 0.0;
    }
    cov / (va * vb).sqrt()
}

/// True when this stretch of mic audio is the call coming out of the
/// speakers rather than you talking.
///
/// `call` is the call's loudness for the same moment, starting
/// `LEAD_FRAMES` earlier and ending `LAG_FRAMES` later than `mic`. Compared
/// on a log scale, since the echo is much quieter but keeps the same shape.
/// When you talk over the call the shapes stop matching, so you are kept.
pub fn is_echo(mic: &[f32], call: &[f32]) -> bool {
    echo_score(mic, call) >= ECHO
}

pub fn echo_score(mic: &[f32], call: &[f32]) -> f32 {
    let level = speech_level(call);
    let talking = call.iter().filter(|&&e| e > level).count();
    if talking * 4 < mic.len() {
        return 0.0; // the call was mostly quiet: whatever the mic heard is you
    }
    let log = |v: &[f32]| v.iter().map(|e| (e + 1e-5).ln()).collect::<Vec<f32>>();
    let (m, c) = (log(mic), log(call));
    (0..=LEAD_FRAMES + LAG_FRAMES)
        .filter_map(|off| c.get(off..off + m.len()))
        .map(|window| pearson(&m, window))
        .fold(0.0, f32::max)
}

// --- speakers ----------------------------------------------------------------

fn normalized(v: &[f32]) -> Vec<f32> {
    let n = v
        .iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
        .max(f32::EPSILON);
    v.iter().map(|x| x / n).collect()
}

/// Group voice fingerprints into speakers (average linkage). Labels are
/// numbered in order of first appearance, so the first voice is always 0.
pub fn cluster(prints: &[Vec<f32>], threshold: f32) -> Vec<u32> {
    let n = prints.len();
    if n == 0 {
        return Vec::new();
    }
    let p: Vec<Vec<f32>> = prints.iter().map(|v| normalized(v)).collect();
    // sum[i][j]: total pairwise similarity between groups i and j.
    let mut sum = vec![vec![0f32; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let s: f32 = p[i].iter().zip(&p[j]).map(|(a, b)| a * b).sum();
            sum[i][j] = s;
            sum[j][i] = s;
        }
    }
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut alive: Vec<bool> = vec![true; n];
    loop {
        let mut best = (f32::MIN, 0, 0);
        for i in 0..n {
            if !alive[i] {
                continue;
            }
            for j in i + 1..n {
                if !alive[j] {
                    continue;
                }
                let avg = sum[i][j] / (members[i].len() * members[j].len()) as f32;
                if avg > best.0 {
                    best = (avg, i, j);
                }
            }
        }
        let (avg, i, j) = best;
        if avg < threshold {
            break;
        }
        alive[j] = false;
        let moved = std::mem::take(&mut members[j]);
        members[i].extend(moved);
        for k in 0..n {
            if k != i && alive[k] {
                sum[i][k] += sum[j][k];
                sum[k][i] = sum[i][k];
            }
        }
    }
    let mut group_of = vec![0usize; n];
    for (g, m) in members.iter().enumerate() {
        for &i in m {
            group_of[i] = g;
        }
    }
    let mut label: Vec<Option<u32>> = vec![None; n];
    let mut next = 0;
    group_of
        .iter()
        .map(|&g| {
            *label[g].get_or_insert_with(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

/// "Speaker A" … "Speaker Z", then "Speaker 27" and on.
pub fn default_name(voice: u32) -> String {
    match char::from_u32('A' as u32 + voice) {
        Some(c) if voice < 26 => format!("Speaker {c}"),
        _ => format!("Speaker {}", voice + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise, so tests don't need a crate.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((self.0 >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        }
    }

    /// Bursts of noise shaped like syllables: `on` and `off` lengths in ms.
    fn speechy(seed: u64, ms: usize, amp: f32) -> Vec<f32> {
        let mut r = Lcg(seed);
        let mut out = Vec::with_capacity(ms * 16);
        while out.len() < ms * 16 {
            let on = 120 + ((r.next().abs() * 250.0) as usize);
            let off = 60 + ((r.next().abs() * 200.0) as usize);
            for i in 0..on * 16 {
                let shape = (std::f32::consts::PI * i as f32 / (on * 16) as f32).sin();
                out.push(r.next() * amp * shape);
            }
            out.extend(std::iter::repeat_n(0.0, off * 16));
        }
        out.truncate(ms * 16);
        out
    }

    fn silence(ms: usize) -> Vec<f32> {
        vec![0.0; ms * 16]
    }

    #[test]
    fn fft_matches_a_known_tone() {
        let mut buf: Vec<(f64, f64)> = (0..512)
            .map(|i| {
                (
                    (2.0 * std::f64::consts::PI * 8.0 * i as f64 / 512.0).cos(),
                    0.0,
                )
            })
            .collect();
        fft(&mut buf);
        let mag = |k: usize| (buf[k].0.powi(2) + buf[k].1.powi(2)).sqrt();
        assert!((mag(8) - 256.0).abs() < 1e-6);
        assert!(mag(7) < 1e-6 && mag(9) < 1e-6);
    }

    #[test]
    fn fbank_has_one_row_per_10ms_and_no_band_offset() {
        let x = speechy(1, 1000, 0.3);
        let (frames, f) = fbank(&x);
        assert_eq!(frames, 1 + (16_000 - 400) / 160);
        assert_eq!(f.len(), frames * MEL);
        for m in 0..MEL {
            let mean: f32 = (0..frames).map(|t| f[t * MEL + m]).sum::<f32>() / frames as f32;
            assert!(mean.abs() < 1e-3, "band {m} mean {mean}");
        }
    }

    #[test]
    fn a_tone_lights_up_the_band_it_sits_in() {
        let x: Vec<f32> = (0..16_000)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 16_000.0).sin())
            .collect();
        // Before mean removal the 1 kHz band is the loudest; check via raw
        // energy by comparing against a silent second half.
        let mut y = x.clone();
        y.extend(silence(1000));
        let (frames, f) = fbank(&y);
        let row = &f[(frames / 4) * MEL..(frames / 4 + 1) * MEL];
        let peak = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap();
        // 1 kHz sits around band 23 of 80 on the mel scale (20 Hz to 8 kHz).
        assert!((20..=27).contains(&peak), "peak band {peak}");
    }

    #[test]
    fn utterances_split_at_pauses_and_keep_short_words_attached() {
        let mut x = speechy(2, 2000, 0.3);
        x.extend(silence(800));
        x.extend(speechy(3, 1500, 0.3));
        x.extend(silence(500));
        x.extend(speechy(4, 800, 0.3)); // too short alone, close to the previous
        let u = utterances(&x);
        assert_eq!(u.len(), 2, "{u:?}");
        assert!(u[0].start < 16 * 100 && u[0].end > 16 * 1700);
        assert!(u[1].end > 16 * 4900, "short tail joined: {u:?}");
        assert!(u.iter().all(Utterance::can_fingerprint));
    }

    #[test]
    fn a_couple_of_clicks_are_not_a_voice() {
        let mut x = silence(1000);
        for _ in 0..2 {
            x.extend(speechy(12, 220, 0.3));
            x.extend(silence(700));
        }
        x.extend(silence(1000));
        assert!(utterances(&x).iter().all(|u| !u.can_fingerprint()));
    }

    #[test]
    fn silence_has_no_utterances() {
        assert!(utterances(&silence(3000)).is_empty());
        let mut hiss = Lcg(9);
        let quiet: Vec<f32> = (0..48_000).map(|_| hiss.next() * 0.001).collect();
        assert!(utterances(&quiet).is_empty());
    }

    /// The mic's view of the call: delayed, much quieter, with room noise.
    fn through_speakers(call: &[f32], delay_ms: usize, seed: u64) -> Vec<f32> {
        let mut r = Lcg(seed);
        let d = delay_ms * 16;
        (0..call.len())
            .map(|i| {
                let echo = if i >= d { call[i - d] * 0.15 } else { 0.0 };
                echo + r.next() * 0.002
            })
            .collect()
    }

    /// Slice the call's loudness the way the meeting does for a mic piece.
    fn windows(mic: &[f32], call: &[f32], from_ms: usize, to_ms: usize) -> (Vec<f32>, Vec<f32>) {
        let (m, c) = (loudness(mic), loudness(call));
        let (a, b) = (from_ms / 10, to_ms / 10);
        (
            m[a..b].to_vec(),
            c[a - LEAD_FRAMES..b + LAG_FRAMES].to_vec(),
        )
    }

    #[test]
    fn the_call_heard_through_the_speakers_is_echo() {
        let mut call = silence(500);
        call.extend(speechy(5, 6000, 0.4));
        call.extend(silence(500));
        let mic = through_speakers(&call, 40, 6);
        let (m, c) = windows(&mic, &call, 1000, 6000);
        assert!(is_echo(&m, &c), "score {}", echo_score(&m, &c));
    }

    #[test]
    fn you_talking_while_the_call_is_quiet_is_kept() {
        let call = silence(7000);
        let mut mic = silence(500);
        mic.extend(speechy(7, 6000, 0.3));
        mic.extend(silence(500));
        let (m, c) = windows(&mic, &call, 1000, 6000);
        assert!(!is_echo(&m, &c));
    }

    #[test]
    fn you_talking_over_the_call_is_kept() {
        let mut call = silence(500);
        call.extend(speechy(8, 6000, 0.4));
        call.extend(silence(500));
        let echo = through_speakers(&call, 40, 9);
        let mut you = silence(500);
        you.extend(speechy(10, 6000, 0.3));
        you.extend(silence(500));
        let mic: Vec<f32> = echo.iter().zip(&you).map(|(a, b)| a + b).collect();
        let (m, c) = windows(&mic, &call, 1000, 6000);
        assert!(!is_echo(&m, &c), "score {}", echo_score(&m, &c));
    }

    #[test]
    fn clusters_find_the_speakers_and_number_them_by_appearance() {
        // Three "voices": directions in 8-D, with a little spread each.
        let base = [
            [1.0, 0.2, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0],
            [0.0, 0.1, 1.0, 0.2, 0.0, 0.0, 0.0, 0.1],
            [0.1, 0.0, 0.0, 0.0, 0.0, 1.0, 0.3, 0.0],
        ];
        let mut r = Lcg(11);
        let order = [1, 1, 0, 2, 1, 0, 2, 2, 0];
        let prints: Vec<Vec<f32>> = order
            .iter()
            .map(|&s| base[s].iter().map(|v| v + r.next() * 0.15).collect())
            .collect();
        assert_eq!(
            cluster(&prints, SAME_SPEAKER),
            vec![0, 0, 1, 2, 0, 1, 2, 2, 1]
        );
    }

    #[test]
    fn one_voice_and_no_voices() {
        assert_eq!(cluster(&[], SAME_SPEAKER), Vec::<u32>::new());
        assert_eq!(cluster(&[vec![1.0, 0.0]], SAME_SPEAKER), vec![0]);
    }

    #[test]
    fn speakers_are_lettered() {
        assert_eq!(default_name(0), "Speaker A");
        assert_eq!(default_name(25), "Speaker Z");
        assert_eq!(default_name(26), "Speaker 27");
    }
}
