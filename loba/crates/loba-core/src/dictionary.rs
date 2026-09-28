//! `dictionary.json` — your words, spelled your way.
//!
//! Each entry is a canonical `term` plus the ways speech recognition tends to
//! mishear it. The terms bias Whisper up front (initial prompt); the
//! mishearings are rewritten afterwards, both before and after the LLM, so a
//! model that "corrects" DAUZE back to "those" gets overruled.

use crate::cleanup::is_han;
use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
pub struct Dictionary {
    #[serde(default)]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
pub struct Entry {
    /// Canonical spelling, pasted exactly as written.
    pub term: String,
    /// What the recogniser writes instead. Matched as whole words,
    /// case-insensitively; a space matches any run of whitespace.
    #[serde(default)]
    pub sounds_like: Vec<String>,
}

/// Whisper's prompt window is ~224 tokens and shared with its own context;
/// past this, terms stop helping and start crowding out the audio.
const PROMPT_BUDGET_CHARS: usize = 600;

impl Dictionary {
    pub fn seed() -> Self {
        Self {
            entries: vec![
                Entry {
                    term: "L.O.B.A".into(),
                    sounds_like: vec!["loba".into(), "lobba".into(), "low ba".into()],
                },
                Entry {
                    term: "DAUZE".into(),
                    sounds_like: vec!["those".into()],
                },
            ],
        }
    }

    /// Trim, drop empties and duplicates, never let a term list itself as a
    /// mishearing of a different term.
    pub fn normalized(self) -> Self {
        let mut seen_terms: Vec<String> = Vec::new();
        let mut entries = Vec::new();
        for e in self.entries {
            let term = e.term.trim().to_string();
            if term.is_empty() || seen_terms.iter().any(|t| t.eq_ignore_ascii_case(&term)) {
                continue;
            }
            seen_terms.push(term.clone());
            let mut sounds_like: Vec<String> = Vec::new();
            for s in e.sounds_like {
                let s = collapse_ws(s.trim());
                if !s.is_empty()
                    && !s.eq_ignore_ascii_case(&term)
                    && !sounds_like.iter().any(|x| x.eq_ignore_ascii_case(&s))
                {
                    sounds_like.push(s);
                }
            }
            entries.push(Entry { term, sounds_like });
        }
        Self { entries }
    }

    /// Terms for the Whisper initial prompt, most recently added first, cut
    /// at the prompt budget.
    pub fn whisper_prompt(&self) -> Option<String> {
        let mut out = String::new();
        for e in self.entries.iter().rev() {
            let add = if out.is_empty() {
                e.term.len()
            } else {
                e.term.len() + 2
            };
            if out.len() + add > PROMPT_BUDGET_CHARS {
                break;
            }
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(&e.term);
        }
        (!out.is_empty()).then_some(out)
    }

    pub fn terms(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.term.clone()).collect()
    }

    /// Rewrite mishearings (and mis-cased terms) to the canonical term.
    pub fn apply(&self, text: &str) -> String {
        let rules = self.rules();
        if rules.is_empty() {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        'scan: while i < chars.len() {
            if at_boundary_start(&chars, i) || is_han(chars[i]) {
                for (pat, term) in &rules {
                    if let Some(end) = match_at(&chars, i, pat) {
                        if pattern_can_end(pat, &chars, end) {
                            out.push_str(term);
                            i = end;
                            continue 'scan;
                        }
                    }
                }
            }
            out.push(chars[i]);
            i += 1;
        }
        out
    }

    /// (pattern chars, replacement), longest pattern first so "low ba" beats
    /// "ba" and a term is never half-replaced.
    fn rules(&self) -> Vec<(Vec<char>, &str)> {
        let mut rules: Vec<(Vec<char>, &str)> = Vec::new();
        for e in &self.entries {
            for s in e.sounds_like.iter().chain(std::iter::once(&e.term)) {
                let pat: Vec<char> = s.chars().collect();
                if !pat.is_empty() {
                    rules.push((pat, e.term.as_str()));
                }
            }
        }
        rules.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        rules
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() && !is_han(c)
}

fn at_boundary_start(chars: &[char], i: usize) -> bool {
    i == 0 || !is_word(chars[i - 1])
}

/// A Latin-ish pattern must end at a word boundary; a Han pattern can end
/// anywhere (Chinese has no spaces to anchor on).
fn pattern_can_end(pat: &[char], chars: &[char], end: usize) -> bool {
    match pat.last() {
        Some(&c) if is_word(c) => end == chars.len() || !is_word(chars[end]),
        _ => true,
    }
}

fn match_at(chars: &[char], start: usize, pat: &[char]) -> Option<usize> {
    // Latin-ish patterns may only start where a word starts.
    if is_word(pat[0]) && !at_boundary_start(chars, start) {
        return None;
    }
    let mut i = start;
    for &p in pat {
        if p == ' ' {
            let ws_start = i;
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            if i == ws_start {
                return None;
            }
            continue;
        }
        let c = *chars.get(i)?;
        if !c.to_lowercase().eq(p.to_lowercase()) {
            return None;
        }
        i += 1;
    }
    Some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(entries: &[(&str, &[&str])]) -> Dictionary {
        Dictionary {
            entries: entries
                .iter()
                .map(|(t, s)| Entry {
                    term: t.to_string(),
                    sounds_like: s.iter().map(|x| x.to_string()).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn replaces_whole_words_case_insensitively() {
        let d = dict(&[("DAUZE", &["those"])]);
        assert_eq!(d.apply("Those are mine"), "DAUZE are mine");
        assert_eq!(d.apply("I like those."), "I like DAUZE.");
        // Not inside other words.
        assert_eq!(d.apply("thoseville"), "thoseville");
    }

    #[test]
    fn normalizes_term_casing() {
        let d = dict(&[("Kubernetes", &["cooper netties"])]);
        assert_eq!(d.apply("deploy to kubernetes"), "deploy to Kubernetes");
        assert_eq!(d.apply("deploy to cooper  netties"), "deploy to Kubernetes");
    }

    #[test]
    fn longest_pattern_wins() {
        let d = dict(&[("L.O.B.A", &["low ba"]), ("BA", &["ba"])]);
        assert_eq!(d.apply("open low ba now"), "open L.O.B.A now");
    }

    #[test]
    fn han_patterns_match_without_spaces() {
        let d = dict(&[("通义千问", &["通一千问"])]);
        assert_eq!(d.apply("我在用通一千问写代码"), "我在用通义千问写代码");
    }

    #[test]
    fn latin_pattern_next_to_han_matches() {
        let d = dict(&[("DAUZE", &["those"])]);
        assert_eq!(d.apply("我喜欢those东西"), "我喜欢DAUZE东西");
    }

    #[test]
    fn replacement_is_not_rescanned() {
        // "DAUZE" contains no pattern, but a term that contains its own
        // mishearing must not loop or double-apply.
        let d = dict(&[("Data Lake", &["data"])]);
        assert_eq!(d.apply("data"), "Data Lake");
    }

    #[test]
    fn normalized_dedupes_and_drops_self_references() {
        let d = dict(&[
            (" DAUZE ", &["those", "Those", "dauze", "  "]),
            ("dauze", &["x"]),
            ("", &["y"]),
        ])
        .normalized();
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.entries[0].term, "DAUZE");
        assert_eq!(d.entries[0].sounds_like, vec!["those"]);
    }

    #[test]
    fn whisper_prompt_is_budgeted_newest_first() {
        let many: Vec<(String, Vec<&str>)> =
            (0..200).map(|i| (format!("Term{i:03}"), vec![])).collect();
        let d = Dictionary {
            entries: many
                .iter()
                .map(|(t, _)| Entry {
                    term: t.clone(),
                    sounds_like: vec![],
                })
                .collect(),
        };
        let p = d.whisper_prompt().unwrap();
        assert!(p.len() <= PROMPT_BUDGET_CHARS);
        assert!(p.starts_with("Term199"));
        assert_eq!(Dictionary::default().whisper_prompt(), None);
    }

    #[test]
    fn user_file_shape_parses() {
        let d: Dictionary = serde_json::from_str(
            r#"{"entries":[{"term":"DAUZE","sounds_like":["those"]},{"term":"Qwen"}]}"#,
        )
        .unwrap();
        assert_eq!(d.entries[1].sounds_like.len(), 0);
    }
}
