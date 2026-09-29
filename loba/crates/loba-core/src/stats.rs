//! `stats.json`: running totals for the Home page. History only keeps the
//! last 50 dictations, so totals can't be derived from it without resetting.

use crate::cleanup::is_han;
use serde::{Deserialize, Serialize};
use specta::Type;

/// Typing speed used for "time saved". A typical typist; the speaking side
/// uses a relaxed dictation pace.
const TYPING_WPM: f64 = 40.0;
const SPEAKING_WPM: f64 = 130.0;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Type)]
#[serde(default)]
pub struct Stats {
    pub dictations: u64,
    pub words: u64,
    /// Local calendar days (YYYY-MM-DD) with at least one dictation, newest
    /// last, capped so the file stays small.
    pub days: Vec<String>,
}

const MAX_DAYS: usize = 400;

impl Stats {
    pub fn record(&mut self, text: &str, today: &str) {
        self.dictations += 1;
        self.words += count_words(text);
        if self.days.last().map(String::as_str) != Some(today) {
            self.days.push(today.to_string());
            if self.days.len() > MAX_DAYS {
                let drop = self.days.len() - MAX_DAYS;
                self.days.drain(..drop);
            }
        }
    }

    /// Minutes saved versus typing the same words.
    pub fn minutes_saved(&self) -> f64 {
        let w = self.words as f64;
        (w / TYPING_WPM - w / SPEAKING_WPM).max(0.0)
    }
}

/// Latin-script words split on whitespace; each Han character counts as one
/// word (Chinese has no spaces to split on). Punctuation-only tokens don't count.
pub fn count_words(text: &str) -> u64 {
    let mut n = 0;
    for token in text.split_whitespace() {
        let han = token.chars().filter(|c| is_han(*c)).count() as u64;
        let has_other_word_chars = token.chars().any(|c| c.is_alphanumeric() && !is_han(c));
        n += han + u64::from(has_other_word_chars);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_mixed_scripts() {
        assert_eq!(count_words("Meet at four."), 3);
        assert_eq!(count_words("我们明天 meeting"), 5);
        assert_eq!(count_words("— , ."), 0);
        assert_eq!(count_words("on se voit à 16h"), 5);
    }

    #[test]
    fn days_are_unique_and_capped() {
        let mut s = Stats::default();
        s.record("a b", "2026-09-29");
        s.record("c", "2026-09-29");
        s.record("d", "2026-09-30");
        assert_eq!(s.dictations, 3);
        assert_eq!(s.words, 4);
        assert_eq!(s.days, vec!["2026-09-29", "2026-09-30"]);
        for i in 0..500 {
            s.record("x", &format!("d{i}"));
        }
        assert_eq!(s.days.len(), MAX_DAYS);
    }

    #[test]
    fn time_saved_is_positive_and_scales() {
        let s = Stats {
            words: 1300,
            ..Default::default()
        };
        let m = s.minutes_saved();
        assert!((m - (32.5 - 10.0)).abs() < 1e-9);
    }
}
