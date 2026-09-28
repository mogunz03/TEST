//! The whole post-transcription pass, in order:
//!
//! 1. dictionary rewrite (so the LLM sees the right spellings)
//! 2. cleanup via Ollama, bounded by the budget
//! 3. dictionary rewrite again (so the LLM can't undo step 1)

use crate::cleanup::{self, Cleaned, CleanupJob, Outcome};
use crate::config::LobaConfig;
use crate::dictionary::Dictionary;
use crate::ollama::OllamaClient;
use crate::prompt::PromptContext;
use std::time::Duration;

pub struct Pipeline<'a> {
    pub client: &'a OllamaClient,
    pub config: &'a LobaConfig,
    pub dictionary: &'a Dictionary,
}

pub async fn run(p: Pipeline<'_>, raw: &str) -> Cleaned {
    let pre = p.dictionary.apply(raw);

    if !p.config.cleanup_enabled {
        return Cleaned {
            text: pre,
            outcome: Outcome::Skipped("cleanup off".into()),
            elapsed_ms: 0,
        };
    }

    let mut out = cleanup::clean(
        CleanupJob {
            client: p.client,
            model: &p.config.model,
            keep_alive: &p.config.keep_alive,
            budget: Duration::from_millis(p.config.timeout_ms),
            context: PromptContext {
                tone: None,
                vocabulary: p.dictionary.terms(),
            },
        },
        &pre,
    )
    .await;
    out.text = p.dictionary.apply(&out.text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dictionary::Entry;

    #[tokio::test]
    async fn dictionary_applies_even_when_llm_is_down() {
        let client = OllamaClient::new("http://127.0.0.1:9");
        let config = LobaConfig::default();
        let dictionary = Dictionary {
            entries: vec![Entry {
                term: "DAUZE".into(),
                sounds_like: vec!["those".into()],
            }],
        };
        let out = run(
            Pipeline {
                client: &client,
                config: &config,
                dictionary: &dictionary,
            },
            "grab those files",
        )
        .await;
        assert_eq!(out.text, "grab DAUZE files");
        assert!(matches!(out.outcome, Outcome::Fallback(_)));
    }

    #[tokio::test]
    async fn cleanup_off_still_applies_dictionary() {
        let client = OllamaClient::new("http://127.0.0.1:9");
        let config = LobaConfig {
            cleanup_enabled: false,
            ..Default::default()
        };
        let dictionary = Dictionary::seed();
        let out = run(
            Pipeline {
                client: &client,
                config: &config,
                dictionary: &dictionary,
            },
            "open loba",
        )
        .await;
        assert_eq!(out.text, "open L.O.B.A");
        assert!(matches!(out.outcome, Outcome::Skipped(_)));
    }
}
