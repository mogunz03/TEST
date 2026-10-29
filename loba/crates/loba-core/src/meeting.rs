//! Meetings: what was said, by whom, and a recap of it.
//!
//! Two audio sources give the first split for free: the microphone is
//! "You", the Mac's own output (the call) is everyone else. With the voice
//! model installed, the call is split further into Speaker A, B, C (see
//! `voices`), and each can be renamed. Audio is cut into chunks at quiet
//! moments, transcribed as it goes, and kept as timestamped segments.
//! When the meeting ends, the local model writes the recap.

use crate::cleanup::strip_think;
use crate::config::write_json;
use crate::ollama::{Message, OllamaClient};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    /// The microphone: the person running L.O.B.A.
    #[default]
    You,
    /// The Mac's audio output: everyone else on the call.
    Others,
}

impl Speaker {
    pub fn label(self) -> &'static str {
        match self {
            Speaker::You => "You",
            Speaker::Others => "Others",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
pub struct Segment {
    /// Offset from the start of the meeting.
    pub start_ms: u64,
    pub speaker: Speaker,
    pub text: String,
    /// Which voice on the call, when voices were told apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
#[serde(default)]
pub struct Meeting {
    pub id: String,
    /// Unix seconds.
    pub started_at: i64,
    pub duration_ms: u64,
    pub segments: Vec<Segment>,
    /// Markdown, once written.
    pub recap: Option<String>,
    pub recap_error: Option<String>,
    /// False when only the microphone could be recorded (no permission for
    /// system audio), so the recap knows "Others" weren't heard.
    pub system_audio: bool,
    /// Names given to voices ("Mia" for voice 0). Unnamed ones read
    /// "Speaker A", "Speaker B"…
    pub names: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Type)]
pub struct MeetingSummary {
    pub id: String,
    pub started_at: i64,
    pub duration_ms: u64,
    pub segments: u32,
    pub has_recap: bool,
    /// First line of the recap's summary, or the first thing said.
    pub preview: String,
}

impl Meeting {
    pub fn summary(&self) -> MeetingSummary {
        let preview = self
            .recap
            .as_deref()
            .and_then(|r| {
                r.lines()
                    .map(str::trim)
                    .find(|l| !l.is_empty() && !l.starts_with('#'))
            })
            .or_else(|| self.segments.first().map(|s| s.text.as_str()))
            .unwrap_or("");
        MeetingSummary {
            id: self.id.clone(),
            started_at: self.started_at,
            duration_ms: self.duration_ms,
            segments: self.segments.len() as u32,
            has_recap: self.recap.is_some(),
            preview: truncate_chars(preview, 140),
        }
    }

    /// What to call whoever said this segment.
    pub fn who(&self, s: &Segment) -> String {
        match (s.speaker, s.voice) {
            (Speaker::Others, Some(v)) => self.voice_name(v),
            (speaker, _) => speaker.label().to_string(),
        }
    }

    pub fn voice_name(&self, voice: u32) -> String {
        self.names
            .get(&voice)
            .cloned()
            .unwrap_or_else(|| crate::voices::default_name(voice))
    }

    /// Give a voice a name, everywhere: the transcript reads it from
    /// `names`, and the recap, already written, has the old name swapped
    /// out so it doesn't need writing again. An empty name goes back to
    /// "Speaker A".
    pub fn rename_voice(&mut self, voice: u32, name: &str) {
        let old = self.voice_name(voice);
        let name = name.trim();
        if name.is_empty() {
            self.names.remove(&voice);
        } else {
            self.names.insert(voice, name.chars().take(40).collect());
        }
        let new = self.voice_name(voice);
        if let Some(r) = &mut self.recap {
            *r = replace_name(r, &old, &new);
        }
    }

    /// Store a freshly written recap. Voices named while it was being
    /// written still appear as "Speaker A" in it, so swap those in.
    pub fn set_recap(&mut self, recap: String) {
        let mut r = recap;
        for (&voice, name) in &self.names {
            r = replace_name(&r, &crate::voices::default_name(voice), name);
        }
        self.recap = Some(r);
        self.recap_error = None;
    }

    /// "[03:12] You: words", one line per segment.
    pub fn transcript(&self) -> String {
        self.segments
            .iter()
            .map(|s| format!("[{}] {}: {}", clock(s.start_ms), self.who(s), s.text.trim()))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The whole meeting as one Markdown document, for copying out.
    pub fn markdown(&self, title: &str) -> String {
        let mut out = format!("# {title}\n\n");
        if let Some(r) = &self.recap {
            out.push_str(r.trim());
            out.push_str("\n\n");
        }
        out.push_str("## Transcript\n\n");
        for s in &self.segments {
            out.push_str(&format!(
                "**{}** {} {}\n\n",
                clock(s.start_ms),
                self.who(s),
                s.text.trim()
            ));
        }
        out
    }
}

/// Swap a speaker's name for another, only where it stands as a whole name
/// ("Speaker A" changes, "Speaker AB" and "Mia's" partly do not).
fn replace_name(text: &str, old: &str, new: &str) -> String {
    if old.is_empty() || old == new {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(old) {
        let before = rest[..at].chars().next_back();
        let after = rest[at + old.len()..].chars().next();
        let whole =
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric);
        out.push_str(&rest[..at]);
        out.push_str(if whole { new } else { old });
        rest = &rest[at + old.len()..];
    }
    out.push_str(rest);
    out
}

pub fn clock(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn truncate_chars(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{}…", t.trim_end())
    } else {
        t
    }
}

// --- storage ---------------------------------------------------------------

/// One JSON file per meeting in `<data>/loba/meetings/`.
pub struct MeetingStore {
    dir: PathBuf,
}

/// Ids name files; anything but letters, digits and dashes is refused so a
/// crafted id can't reach outside the folder.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl MeetingStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> Option<PathBuf> {
        valid_id(id).then(|| self.dir.join(format!("{id}.json")))
    }

    pub fn save(&self, m: &Meeting) -> Result<(), String> {
        let path = self.path(&m.id).ok_or("bad meeting id")?;
        write_json(&path, m).map_err(|e| e.to_string())
    }

    pub fn load(&self, id: &str) -> Result<Meeting, String> {
        let path = self.path(id).ok_or("bad meeting id")?;
        let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path(id).ok_or("bad meeting id")?;
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }

    /// Newest first. Unreadable files are skipped, not fatal.
    pub fn list(&self) -> Vec<MeetingSummary> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut out: Vec<MeetingSummary> = entries
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| std::fs::read_to_string(e.path()).ok())
            .filter_map(|raw| serde_json::from_str::<Meeting>(&raw).ok())
            .map(|m| m.summary())
            .collect();
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        out
    }
}

// --- audio chunking --------------------------------------------------------

pub const SAMPLE_RATE: usize = 16_000;
/// Don't cut before this much audio: Whisper does better with context.
pub const CHUNK_MIN: usize = 8 * SAMPLE_RATE;
/// Cut here even mid-sentence, so text keeps arriving during a monologue.
pub const CHUNK_MAX: usize = 25 * SAMPLE_RATE;

const FRAME: usize = SAMPLE_RATE * 30 / 1000;
const QUIET_WINDOW: usize = SAMPLE_RATE * 300 / 1000;
const QUIET_STEP: usize = SAMPLE_RATE / 10;

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

/// Whether a chunk holds enough voice to be worth transcribing. Whisper
/// invents text ("Thank you.") on silence, so near-silent chunks are dropped.
pub fn has_speech(x: &[f32]) -> bool {
    let voiced = x.chunks(FRAME).filter(|f| rms(f) > 0.01).count();
    voiced * FRAME >= SAMPLE_RATE * 6 / 10
}

/// Where to cut the buffer: the latest quiet 300 ms after `CHUNK_MIN`, or
/// `CHUNK_MAX` if nobody pauses. `None` means keep listening.
pub fn cut_point(buf: &[f32]) -> Option<usize> {
    if buf.len() < CHUNK_MIN {
        return None;
    }
    let quiet = (0.004f32).max(rms(buf) * 0.15);
    let mut at = buf.len().saturating_sub(QUIET_WINDOW);
    while at >= CHUNK_MIN {
        if rms(&buf[at..at + QUIET_WINDOW]) < quiet {
            return Some(at + QUIET_WINDOW / 2);
        }
        at -= QUIET_STEP;
    }
    (buf.len() >= CHUNK_MAX).then_some(buf.len())
}

// --- recap -----------------------------------------------------------------

pub const RECAP_RULES: &str = "\
You write the recap of a meeting from its transcript, or from notes taken on each part of it. \
Transcript lines look like \"[time] Speaker: words\". \"You\" is the person who recorded the meeting. \
The others on the call appear by name, as \"Speaker A\", \"Speaker B\" and so on, or all together as \"Others\". \
Use the speaker labels exactly as written, even when the conversation suggests a name: \
the person who recorded the meeting renames speakers themselves.

Write the recap in the language most of the meeting was held in, as Markdown with these sections. \
Leave out any section that would be empty.

## Summary
Three to six sentences: what the meeting was about and where it landed.

## Decisions
One bullet per decision that was actually made.

## Action items
One bullet per task, as \"- **Who**: task (by when)\". Only add a date if one was said.

## Open questions
Things raised but not settled.

Rules: use only what is in the text. Never invent names, numbers, dates or decisions. \
Speech recognition makes mistakes: when a word is clearly misheard, write the obvious intended word. \
No preamble, no closing remarks.";

pub const NOTES_RULES: &str = "\
This is one part of a longer meeting transcript. Lines look like \"[time] Speaker: words\". \
Write compact bullet notes of this part: topics, decisions, tasks with who and when, open questions, \
and any numbers, names and dates exactly as said. Use only what is in the text. \
Write in the language spoken. Output only the bullets.";

/// Characters per model call. Comfortably inside the context window below
/// for Latin and Han text alike.
const PART_CHARS: usize = 12_000;
const NUM_CTX: i32 = 16_384;
const RECAP_TOKENS: i32 = 1_500;
const CALL_BUDGET: Duration = Duration::from_secs(300);

/// Split on line boundaries into parts of at most `max` characters.
pub fn split_parts(text: &str, max: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if !cur.is_empty() && cur.chars().count() + line.chars().count() + 1 > max {
            parts.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(line);
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

async fn ask(
    client: &OllamaClient,
    model: &str,
    keep_alive: &str,
    rules: &str,
    input: String,
) -> Result<String, String> {
    let messages = [Message::system(rules), Message::user(input)];
    let answer = client
        .chat_with_context(
            model,
            &messages,
            keep_alive,
            RECAP_TOKENS,
            Some(NUM_CTX),
            CALL_BUDGET,
        )
        .await
        .map_err(|e| e.to_string())?;
    let text = strip_think(&answer).trim().to_string();
    if text.is_empty() {
        return Err("the model returned nothing".into());
    }
    Ok(text)
}

/// Write the recap. Long meetings are summarized part by part first, then
/// the notes are recapped, so nothing falls outside the model's window.
pub async fn recap(
    client: &OllamaClient,
    model: &str,
    keep_alive: &str,
    meeting: &Meeting,
) -> Result<String, String> {
    let transcript = meeting.transcript();
    let words = transcript.split_whitespace().count()
        + transcript
            .chars()
            .filter(|c| crate::cleanup::is_han(*c))
            .count();
    if words < 12 {
        return Err("too little was said to recap".into());
    }
    let mut lead = String::new();
    if !meeting.system_audio {
        lead.push_str(
            "(Only the microphone was recorded, so other people on the call are missing.)\n\n",
        );
    }
    let parts = split_parts(&transcript, PART_CHARS);
    if parts.len() == 1 {
        return ask(
            client,
            model,
            keep_alive,
            RECAP_RULES,
            format!("{lead}{transcript}"),
        )
        .await;
    }
    let mut notes = Vec::with_capacity(parts.len());
    for (i, part) in parts.into_iter().enumerate() {
        let n = ask(client, model, keep_alive, NOTES_RULES, part).await?;
        notes.push(format!("Part {}:\n{n}", i + 1));
    }
    ask(
        client,
        model,
        keep_alive,
        RECAP_RULES,
        format!(
            "{lead}Notes from each part of the meeting, in order:\n\n{}",
            notes.join("\n\n")
        ),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(ms: u64, speaker: Speaker, text: &str) -> Segment {
        Segment {
            start_ms: ms,
            speaker,
            text: text.into(),
            voice: None,
        }
    }

    #[test]
    fn voices_read_as_letters_until_named() {
        let mut m = sample();
        m.segments[0].voice = Some(0);
        m.segments[2].voice = Some(1);
        m.recap = Some("- **Speaker A**: send the deck. Speaker AB is someone else.".into());
        assert!(m.transcript().contains("Speaker A: Can we ship on Friday?"));
        assert!(m.transcript().contains("Speaker B: Great."));

        m.rename_voice(0, "  Mia ");
        assert!(m.transcript().contains("Mia: Can we ship on Friday?"));
        assert_eq!(
            m.recap.as_deref(),
            Some("- **Mia**: send the deck. Speaker AB is someone else.")
        );
        // renaming again swaps the name just given, and blank goes back
        m.rename_voice(0, "Mia Chen");
        assert!(m.recap.as_deref().unwrap().contains("**Mia Chen**"));
        m.rename_voice(0, "");
        assert!(m.recap.as_deref().unwrap().contains("**Speaker A**"));
        assert!(m.names.is_empty());
    }

    #[test]
    fn a_recap_written_before_a_rename_gets_the_name() {
        let mut m = sample();
        m.names.insert(1, "Mia".into());
        m.recap_error = Some("old".into());
        m.set_recap("- **Speaker B**: fix the bugs. Speaker A: nothing.".into());
        assert_eq!(
            m.recap.as_deref(),
            Some("- **Mia**: fix the bugs. Speaker A: nothing.")
        );
        assert_eq!(m.recap_error, None);
    }

    #[test]
    fn others_without_a_voice_stay_others() {
        let m = sample();
        assert!(m.transcript().contains("Others: Can we ship on Friday?"));
    }

    #[test]
    fn meetings_saved_before_voices_still_load() {
        let old = r#"{"id":"x","started_at":1,"duration_ms":2,"segments":[{"start_ms":0,"speaker":"others","text":"hi"}],"recap":null,"recap_error":null,"system_audio":true}"#;
        let m: Meeting = serde_json::from_str(old).unwrap();
        assert_eq!(m.segments[0].voice, None);
        assert!(m.names.is_empty());
        let round: Meeting = serde_json::from_str(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(round, m);
    }

    fn sample() -> Meeting {
        Meeting {
            id: "20261027-0915".into(),
            started_at: 1_793_000_000,
            duration_ms: 3_725_000,
            segments: vec![
                seg(0, Speaker::Others, "Can we ship on Friday?"),
                seg(192_000, Speaker::You, " Yes, if QA signs off Thursday. "),
                seg(3_700_000, Speaker::Others, "Great."),
            ],
            system_audio: true,
            ..Default::default()
        }
    }

    #[test]
    fn transcript_lines_carry_time_and_speaker() {
        let t = sample().transcript();
        assert_eq!(
            t,
            "[00:00] Others: Can we ship on Friday?\n[03:12] You: Yes, if QA signs off Thursday.\n[1:01:40] Others: Great."
        );
    }

    #[test]
    fn summary_prefers_recap_text() {
        let mut m = sample();
        assert_eq!(m.summary().preview, "Can we ship on Friday?");
        m.recap = Some("## Summary\n\nShip date agreed for Friday.\n".into());
        assert_eq!(m.summary().preview, "Ship date agreed for Friday.");
        assert!(m.summary().has_recap);
    }

    #[test]
    fn store_round_trips_lists_newest_first_and_refuses_bad_ids() {
        let dir = tempfile::tempdir().unwrap();
        let store = MeetingStore::new(dir.path().join("meetings"));
        let a = sample();
        let mut b = sample();
        b.id = "20261028-1000".into();
        b.started_at += 86_400;
        store.save(&a).unwrap();
        store.save(&b).unwrap();
        assert_eq!(store.load(&a.id).unwrap(), a);
        let ids: Vec<_> = store.list().into_iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![b.id.clone(), a.id.clone()]);
        assert!(store.load("../etc/passwd").is_err());
        assert!(!valid_id("a/b"));
        store.delete(&a.id).unwrap();
        assert_eq!(store.list().len(), 1);
    }

    fn tone(secs: f32, amp: f32) -> Vec<f32> {
        (0..(secs * SAMPLE_RATE as f32) as usize)
            .map(|i| amp * (i as f32 * 0.05).sin())
            .collect()
    }

    #[test]
    fn silence_is_not_speech() {
        assert!(!has_speech(&vec![0.0; SAMPLE_RATE * 5]));
        assert!(has_speech(&tone(2.0, 0.2)));
    }

    #[test]
    fn cuts_at_a_pause_or_at_the_ceiling() {
        assert_eq!(cut_point(&tone(5.0, 0.2)), None);
        let mut buf = tone(10.0, 0.2);
        buf.extend(vec![0.0; SAMPLE_RATE]);
        buf.extend(tone(1.0, 0.2));
        let at = cut_point(&buf).unwrap();
        assert!(
            at > 10 * SAMPLE_RATE && at < 11 * SAMPLE_RATE,
            "cut at {at}"
        );
        assert_eq!(cut_point(&tone(20.0, 0.2)), None);
        assert_eq!(cut_point(&tone(26.0, 0.2)), Some(26 * SAMPLE_RATE));
    }

    #[test]
    fn splits_long_transcripts_on_lines() {
        let text = (0..100)
            .map(|i| format!("[00:{i:02}] You: line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let parts = split_parts(&text, 300);
        assert!(parts.len() > 1);
        assert!(parts.iter().all(|p| p.chars().count() <= 300));
        assert_eq!(parts.join("\n"), text);
    }

    async fn fake_ollama(
        answer: &'static str,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut seen = Vec::new();
                let mut buf = vec![0u8; 65536];
                loop {
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    seen.extend_from_slice(&buf[..n]);
                    let s = String::from_utf8_lossy(&seen);
                    if let Some(h) = s.find("\r\n\r\n") {
                        let len = s[..h]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap())
                            })
                            .unwrap_or(0);
                        if seen.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&seen).into_owned());
                let body = format!(
                    r#"{{"message":{{"role":"assistant","content":{}}},"done":true}}"#,
                    serde_json::to_string(answer).unwrap()
                );
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), rx)
    }

    #[tokio::test]
    async fn recap_sends_the_transcript_with_a_large_context() {
        let (url, mut rx) = fake_ollama("## Summary\n\nShip Friday.").await;
        let client = OllamaClient::new(&url);
        let mut m = sample();
        m.segments.push(seg(
            10_000,
            Speaker::You,
            "QA has the build since Monday and found two small bugs so far",
        ));
        let r = recap(&client, "qwen3:8b", "5m", &m).await.unwrap();
        assert_eq!(r, "## Summary\n\nShip Friday.");
        let req = rx.recv().await.unwrap();
        assert!(req.contains("\"num_ctx\":16384"));
        assert!(req.contains("[03:12] You: Yes, if QA signs off Thursday."));
        assert!(rx.try_recv().is_err(), "one call for a short meeting");
    }

    #[tokio::test]
    async fn long_meetings_are_noted_part_by_part_then_recapped() {
        let (url, mut rx) = fake_ollama("- noted").await;
        let client = OllamaClient::new(&url);
        let mut m = sample();
        m.segments = (0..900)
            .map(|i| {
                seg(
                    i * 4000,
                    Speaker::Others,
                    "we went through the roadmap item by item and agreed",
                )
            })
            .collect();
        recap(&client, "qwen3:8b", "5m", &m).await.unwrap();
        let mut calls = Vec::new();
        while let Ok(r) = rx.try_recv() {
            calls.push(r);
        }
        assert!(
            calls.len() >= 3,
            "parts then a final recap, got {}",
            calls.len()
        );
        assert!(calls.last().unwrap().contains("Notes from each part"));
    }

    #[tokio::test]
    async fn too_short_is_an_error_not_a_call() {
        let client = OllamaClient::new("http://127.0.0.1:9");
        let m = Meeting {
            segments: vec![seg(0, Speaker::You, "Hi")],
            ..Default::default()
        };
        assert!(recap(&client, "qwen3:8b", "5m", &m).await.is_err());
    }
}
