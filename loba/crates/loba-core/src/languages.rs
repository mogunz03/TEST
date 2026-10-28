//! Keeping Whisper inside the languages the user actually speaks.
//!
//! Whisper guesses one language per recording from the first seconds of
//! audio. With an accent, a short clip or a mixed sentence it guesses wrong
//! (French heard as Portuguese, Mandarin heard as Japanese), and everything
//! after that, including cleanup, works on the wrong text. When the user has
//! said which languages they speak, a guess outside that list is overruled
//! and the recording is transcribed again in the closest one they do speak.

/// The languages offered in Settings, in the order shown. Codes are Whisper's.
pub const OFFERED: &[&str] = &["en", "fr", "zh", "es", "de", "it", "pt", "ja", "ko"];

const CJK: &[&str] = &["zh", "yue", "ja", "ko"];
const ROMANCE: &[&str] = &["fr", "es", "it", "pt", "ca", "ro", "gl", "oc", "la"];
const GERMANIC: &[&str] = &[
    "en", "de", "nl", "af", "da", "sv", "no", "nn", "is", "lb", "yi",
];

/// Whisper says "zh" for Mandarin, but some models and settings use regional
/// tags. Compare on the base code.
fn base(code: &str) -> String {
    let lower = code.trim().to_ascii_lowercase();
    let b = lower.split(['-', '_']).next().unwrap_or("").to_string();
    match b.as_str() {
        "cmn" => "zh".into(),
        _ => b,
    }
}

fn family(code: &str) -> Option<&'static [&'static str]> {
    [CJK, ROMANCE, GERMANIC]
        .into_iter()
        .find(|f| f.contains(&code))
}

fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c as u32,
            0x3040..=0x30FF   // kana
            | 0x3400..=0x4DBF // CJK ext A
            | 0x4E00..=0x9FFF // CJK unified
            | 0xAC00..=0xD7AF // hangul
        )
    })
}

/// Clean a user's list: base codes, no blanks, no repeats, order kept.
pub fn normalize(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for code in list.iter().map(|c| base(c)) {
        if !code.is_empty() && code.len() <= 3 && !out.contains(&code) {
            out.push(code);
        }
    }
    out
}

/// With exactly one language chosen there is nothing to detect: tell the
/// model up front instead of guessing and re-running.
pub fn only(spoken: &[String]) -> Option<String> {
    match normalize(spoken).as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Whisper heard `detected` and wrote `text`. If that language is not one
/// the user speaks, return the one to transcribe in instead.
///
/// Closest means: same writing system first (Chinese characters stay
/// Chinese), then same family (Portuguese → French, Dutch → English), then
/// the first non-CJK language on the list.
pub fn steer(detected: &str, text: &str, spoken: &[String]) -> Option<String> {
    let spoken = normalize(spoken);
    if spoken.is_empty() {
        return None;
    }
    let detected = base(detected);
    if spoken.contains(&detected) {
        return None;
    }
    if spoken.len() == 1 {
        return Some(spoken[0].clone());
    }
    let is_cjk = |c: &String| CJK.contains(&c.as_str());

    if CJK.contains(&detected.as_str()) || has_cjk(text) {
        if let Some(c) = spoken.iter().find(|c| is_cjk(c)) {
            return Some(c.clone());
        }
    }
    if let Some(f) = family(&detected) {
        if let Some(c) = spoken.iter().find(|c| f.contains(&c.as_str())) {
            return Some(c.clone());
        }
    }
    spoken
        .iter()
        .find(|c| !is_cjk(c))
        .or_else(|| spoken.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    const MINE: &[&str] = &["en", "fr", "zh"];

    #[test]
    fn nothing_chosen_means_trust_whisper() {
        assert_eq!(steer("pt", "olá", &[]), None);
    }

    #[test]
    fn a_language_you_speak_is_left_alone() {
        assert_eq!(steer("fr", "bonjour", &v(MINE)), None);
        assert_eq!(steer("zh", "你好", &v(MINE)), None);
    }

    #[test]
    fn regional_tags_count_as_the_same_language() {
        assert_eq!(steer("zh-CN", "你好", &v(MINE)), None);
        assert_eq!(steer("en", "hi", &v(&["EN-us"])), None);
    }

    #[test]
    fn romance_mistakes_go_to_french() {
        assert_eq!(steer("pt", "je pense que", &v(MINE)).as_deref(), Some("fr"));
        assert_eq!(steer("it", "on y va", &v(MINE)).as_deref(), Some("fr"));
    }

    #[test]
    fn germanic_mistakes_go_to_english() {
        assert_eq!(steer("nl", "let's go", &v(MINE)).as_deref(), Some("en"));
    }

    #[test]
    fn chinese_characters_stay_chinese() {
        assert_eq!(steer("ja", "我们走吧", &v(MINE)).as_deref(), Some("zh"));
        // detected as a Latin language, but the text is in hanzi
        assert_eq!(steer("pt", "我们走吧", &v(MINE)).as_deref(), Some("zh"));
    }

    #[test]
    fn unrelated_guesses_fall_back_to_the_first_latin_language() {
        assert_eq!(steer("ru", "da", &v(MINE)).as_deref(), Some("en"));
        assert_eq!(steer("ru", "da", &v(&["zh", "fr"])).as_deref(), Some("fr"));
    }

    #[test]
    fn one_language_is_used_directly() {
        assert_eq!(only(&v(&["fr"])).as_deref(), Some("fr"));
        assert_eq!(only(&v(MINE)), None);
        assert_eq!(steer("en", "hello", &v(&["fr"])).as_deref(), Some("fr"));
    }

    #[test]
    fn lists_are_cleaned() {
        assert_eq!(
            normalize(&v(&["EN", "fr-FR", "", "en", "cmn"])),
            v(&["en", "fr", "zh"])
        );
    }
}
