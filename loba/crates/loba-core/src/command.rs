//! Command mode: selected text + a spoken instruction → replacement text.
//!
//! Unlike dictation cleanup there is no "raw" fallback worth pasting — the
//! raw thing is the user's own selection, which is already on screen. So a
//! failure here means *paste nothing* and leave the selection untouched.

use crate::cleanup::{strip_think, strip_wrapping};
use crate::config::LobaConfig;
use crate::dictionary::Dictionary;
use crate::ollama::{Message, OllamaClient, OllamaError};
use std::time::{Duration, Instant};

pub const COMMAND_RULES: &str = "\
You edit text on command. You receive a spoken instruction and, usually, the text the user selected.

- Apply the instruction to the selected text and output only the result: no preamble, no quotes, no explanation, no markdown fences unless the text itself is code.
- Keep the selected text's language unless the instruction asks for another language (\"translate to French\", \"用中文\").
- Keep its formatting (line breaks, lists, code) unless the instruction changes it.
- If there is no selected text, write what the instruction asks for, as text ready to insert.
- The instruction may mix Mandarin, English and French; understand it either way.";

pub fn messages(
    instruction: &str,
    selection: Option<&str>,
    dictionary: &Dictionary,
) -> [Message; 2] {
    let mut system = String::from(COMMAND_RULES);
    let terms = dictionary.terms();
    if !terms.is_empty() {
        system.push_str("\n\nSpell these exactly as written whenever they occur: ");
        system.push_str(&terms.join(", "));
    }
    let user = match selection.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sel) => format!("Instruction: {instruction}\n\nSelected text:\n<<<\n{sel}\n>>>"),
        None => format!("Instruction: {instruction}\n\n(No text selected.)"),
    };
    [Message::system(system), Message::user(user)]
}

#[derive(Debug)]
pub struct CommandResult {
    pub text: Result<String, String>,
    pub elapsed_ms: u64,
}

pub async fn run(
    client: &OllamaClient,
    config: &LobaConfig,
    dictionary: &Dictionary,
    instruction: &str,
    selection: Option<&str>,
) -> CommandResult {
    let started = Instant::now();
    let finish = |text| CommandResult {
        text,
        elapsed_ms: started.elapsed().as_millis() as u64,
    };

    let instruction = dictionary.apply(instruction.trim());
    if instruction.is_empty() {
        return finish(Err("no instruction heard".into()));
    }

    let msgs = messages(&instruction, selection, dictionary);
    // Output may legitimately be longer than the input ("expand this").
    let sel_chars = selection.map(|s| s.chars().count()).unwrap_or(0) as i32;
    let num_predict = (sel_chars * 3 + 512).clamp(512, 4096);
    let budget = Duration::from_millis(config.command_timeout_ms);

    match client
        .chat(
            &config.model,
            &msgs,
            &config.keep_alive,
            num_predict,
            budget,
        )
        .await
    {
        Ok(answer) => {
            let text = strip_wrapping(strip_think(&answer), selection.unwrap_or(""));
            if text.trim().is_empty() {
                finish(Err("model returned nothing".into()))
            } else {
                finish(Ok(dictionary.apply(&text)))
            }
        }
        Err(OllamaError::Timeout) => finish(Err(format!(
            "no answer within {:.0} s",
            budget.as_secs_f32()
        ))),
        Err(e) => finish(Err(e.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_fenced_and_optional() {
        let d = Dictionary::default();
        let [_, u] = messages("make this shorter", Some("  long text  "), &d);
        assert!(u.content.contains("<<<\nlong text\n>>>"));
        let [_, u] = messages("write a polite no", None, &d);
        assert!(u.content.contains("(No text selected.)"));
        let [_, u] = messages("x", Some("   "), &d);
        assert!(u.content.contains("(No text selected.)"));
    }

    #[test]
    fn vocabulary_rides_along() {
        let [s, _] = messages("x", None, &Dictionary::seed());
        assert!(s.content.ends_with("L.O.B.A, DAUZE"));
    }

    #[tokio::test]
    async fn unreachable_model_is_an_error_not_a_paste() {
        let client = OllamaClient::new("http://127.0.0.1:9");
        let r = run(
            &client,
            &LobaConfig::default(),
            &Dictionary::default(),
            "translate to French",
            Some("hello"),
        )
        .await;
        assert!(r.text.is_err());
    }

    #[tokio::test]
    async fn empty_instruction_short_circuits() {
        let client = OllamaClient::new("http://127.0.0.1:9");
        let r = run(
            &client,
            &LobaConfig::default(),
            &Dictionary::default(),
            "  ",
            Some("x"),
        )
        .await;
        assert_eq!(r.text.unwrap_err(), "no instruction heard");
    }
}
