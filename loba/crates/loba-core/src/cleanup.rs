//! Raw transcript -> cleaned text, with the raw transcript as the guaranteed
//! floor. Every failure path (timeout, server down, suspicious answer) returns
//! the input unchanged; nothing here can hold the paste past the budget.

use crate::ollama::{Message, OllamaClient};
use crate::prompt::{cleanup_num_predict, cleanup_system_prompt, PromptContext};
use serde::Serialize;
use specta::Type;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum Outcome {
    /// LLM answer accepted.
    Cleaned,
    /// Deliberately not sent to the LLM (disabled, empty, literal style).
    Skipped(String),
    /// Budget exceeded; raw transcript used.
    TimedOut,
    /// LLM call failed or its answer was rejected; raw transcript used.
    Fallback(String),
}

#[derive(Debug, Clone)]
pub struct Cleaned {
    pub text: String,
    pub outcome: Outcome,
    pub elapsed_ms: u64,
}

pub struct CleanupJob<'a> {
    pub client: &'a OllamaClient,
    pub model: &'a str,
    pub keep_alive: &'a str,
    pub budget: Duration,
    pub context: PromptContext,
}

pub async fn clean(job: CleanupJob<'_>, raw: &str) -> Cleaned {
    let started = Instant::now();
    let done = |text: String, outcome| Cleaned {
        text,
        outcome,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    if raw.trim().is_empty() {
        return done(raw.to_string(), Outcome::Skipped("empty transcript".into()));
    }

    let messages = [
        Message::system(cleanup_system_prompt(&job.context)),
        Message::user(raw),
    ];
    let reply = job
        .client
        .chat(
            job.model,
            &messages,
            job.keep_alive,
            cleanup_num_predict(raw),
            job.budget,
        )
        .await;

    match reply {
        Ok(answer) => match accept(raw, &answer) {
            Ok(text) => done(text, Outcome::Cleaned),
            Err(why) => {
                log::warn!("Cleanup answer rejected: {why}");
                done(raw.to_string(), Outcome::Fallback(why))
            }
        },
        Err(crate::ollama::OllamaError::Timeout) => {
            log::warn!("Cleanup exceeded {:?}; pasting raw transcript", job.budget);
            done(raw.to_string(), Outcome::TimedOut)
        }
        Err(e) => {
            log::warn!("Cleanup failed: {e}; pasting raw transcript");
            done(raw.to_string(), Outcome::Fallback(e.to_string()))
        }
    }
}

/// Normalise the model's answer and decide whether to trust it. A cleanup
/// pass only ever removes or reshapes; anything that looks like it answered,
/// padded, or translated is thrown away in favour of the raw words.
pub fn accept(raw: &str, answer: &str) -> Result<String, String> {
    let text = strip_wrapping(strip_think(answer), raw);
    if text.is_empty() {
        return Err("empty answer".into());
    }

    let raw_len = raw.chars().count();
    let out_len = text.chars().count();
    // Lists add "- " and newlines; beyond that, growth means invented content.
    if out_len > raw_len * 3 / 2 + 40 {
        return Err(format!("answer grew {raw_len} -> {out_len} chars"));
    }

    let lower = text.to_lowercase();
    let raw_lower = raw.trim_start().to_lowercase();
    const PREAMBLES: [&str; 8] = [
        "here is",
        "here's",
        "sure,",
        "cleaned text",
        "以下是",
        "好的，",
        "voici",
        "bien sûr",
    ];
    // Only a preamble if the speaker didn't open with those words themselves.
    if PREAMBLES
        .iter()
        .any(|p| lower.starts_with(p) && !raw_lower.starts_with(p))
    {
        return Err("answer has a preamble".into());
    }

    // "Never translate": the Han share of the text must survive the pass.
    let (raw_han, raw_letters) = script_counts(raw);
    let (out_han, out_letters) = script_counts(&text);
    if raw_letters > 0 && out_letters > 0 {
        let raw_ratio = raw_han as f32 / raw_letters as f32;
        let out_ratio = out_han as f32 / out_letters as f32;
        if (raw_ratio - out_ratio).abs() > 0.35 {
            return Err(format!(
                "script mix changed ({:.0}% -> {:.0}% Han); looks translated",
                raw_ratio * 100.0,
                out_ratio * 100.0
            ));
        }
    }

    Ok(text)
}

fn strip_think(s: &str) -> &str {
    let t = s.trim_start();
    if let Some(rest) = t.strip_prefix("<think>") {
        if let Some(end) = rest.find("</think>") {
            return rest[end + "</think>".len()..].trim();
        }
        return "";
    }
    t.trim()
}

/// Drop quotes or a code fence the model wrapped around the whole answer,
/// unless the speaker's own words were quoted that way.
fn strip_wrapping(s: &str, raw: &str) -> String {
    let mut t = s.trim();
    if let Some(inner) = t.strip_prefix("```").and_then(|r| r.strip_suffix("```")) {
        t = inner
            .trim_start_matches(|c: char| c.is_ascii_alphanumeric())
            .trim();
    }
    for (open, close) in [('"', '"'), ('“', '”'), ('「', '」'), ('«', '»')] {
        let raw_quoted = raw.trim_start().starts_with(open);
        if !raw_quoted && t.starts_with(open) && t.ends_with(close) && t.chars().count() >= 2 {
            let inner: String = {
                let mut c = t.chars();
                c.next();
                c.next_back();
                c.collect()
            };
            if !inner.contains(open) {
                return inner.trim().to_string();
            }
        }
    }
    t.to_string()
}

/// (Han ideographs, all letters incl. Han) — punctuation and digits ignored.
fn script_counts(s: &str) -> (usize, usize) {
    s.chars().fold((0, 0), |(han, letters), c| {
        if is_han(c) {
            (han + 1, letters + 1)
        } else if c.is_alphabetic() {
            (han, letters + 1)
        } else {
            (han, letters)
        }
    })
}

pub(crate) fn is_han(c: char) -> bool {
    matches!(c as u32,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x2FA1F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_cleanup() {
        let raw = "um so we should uh meet at three no wait I mean four";
        assert_eq!(
            accept(raw, "We should meet at four.").unwrap(),
            "We should meet at four."
        );
    }

    #[test]
    fn strips_think_and_quotes() {
        let a = "<think>\nok\n</think>\n\n\"Meet at four.\"";
        assert_eq!(accept("meet at four", a).unwrap(), "Meet at four.");
    }

    #[test]
    fn keeps_speaker_quotes() {
        let raw = "\"hello\" is what she said";
        assert_eq!(
            accept(raw, "\"Hello\" is what she said.").unwrap(),
            "\"Hello\" is what she said."
        );
    }

    #[test]
    fn rejects_growth() {
        let raw = "send the file";
        let a = "Send the file. Also, don't forget to include the quarterly numbers and a summary for the board.";
        assert!(accept(raw, a).is_err());
    }

    #[test]
    fn allows_list_formatting_overhead() {
        let raw = "buy milk eggs bread and coffee";
        let a = "Buy:\n- milk\n- eggs\n- bread\n- coffee";
        assert!(accept(raw, a).is_ok());
    }

    #[test]
    fn rejects_preamble() {
        assert!(accept("meet at four", "Here is the cleaned text: Meet at four.").is_err());
    }

    #[test]
    fn rejects_translation() {
        let raw = "我们明天 meeting 的时候讨论一下这个 budget";
        assert!(accept(raw, "We will discuss this budget at tomorrow's meeting.").is_err());
        assert!(accept(raw, "我们明天 meeting 的时候讨论一下这个 budget。").is_ok());
    }

    #[test]
    fn keeps_french_english_mix() {
        let raw = "euh on fait le standup à dix heures ben after that we ship";
        assert!(accept(raw, "On fait le standup à dix heures, after that we ship.").is_ok());
    }

    #[test]
    fn unterminated_think_is_empty() {
        assert!(accept("hi", "<think>still thinking").is_err());
    }

    #[tokio::test]
    async fn unreachable_server_falls_back_fast() {
        // Port 9 (discard) on loopback: connection refused immediately.
        let client = OllamaClient::new("http://127.0.0.1:9");
        let out = clean(
            CleanupJob {
                client: &client,
                model: "qwen3:8b",
                keep_alive: "1m",
                budget: Duration::from_secs(4),
                context: PromptContext::default(),
            },
            "um hello",
        )
        .await;
        assert_eq!(out.text, "um hello");
        assert!(matches!(out.outcome, Outcome::Fallback(_)));
        assert!(out.elapsed_ms < 1000);
    }

    #[tokio::test]
    async fn slow_server_hits_budget() {
        // Accepts the connection, never answers.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (_sock, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let client = OllamaClient::new(&format!("http://{addr}"));
        let started = Instant::now();
        let out = clean(
            CleanupJob {
                client: &client,
                model: "qwen3:8b",
                keep_alive: "1m",
                budget: Duration::from_millis(300),
                context: PromptContext::default(),
            },
            "raw words",
        )
        .await;
        assert_eq!(out.text, "raw words");
        assert_eq!(out.outcome, Outcome::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(800));
    }

    #[tokio::test]
    async fn happy_path_against_fake_ollama() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 16384];
            let n = sock.read(&mut buf).await.unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("\"think\":false"));
            let body = r#"{"message":{"role":"assistant","content":"Meet at four."},"done":true}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            sock.write_all(resp.as_bytes()).await.unwrap();
        });
        let client = OllamaClient::new(&format!("http://{addr}"));
        let out = clean(
            CleanupJob {
                client: &client,
                model: "qwen3:8b",
                keep_alive: "1m",
                budget: Duration::from_secs(4),
                context: PromptContext::default(),
            },
            "um meet at three no wait four",
        )
        .await;
        assert_eq!(out.outcome, Outcome::Cleaned);
        assert_eq!(out.text, "Meet at four.");
    }
}
