//! The whole post-transcription pass, in order:
//!
//! 1. dictionary rewrite (so the LLM sees the right spellings)
//! 2. cleanup via Ollama in the destination app's tone, bounded by the
//!    budget — skipped entirely for literal destinations
//! 3. dictionary rewrite again (so the LLM can't undo step 1)

use crate::cleanup::{self, Cleaned, CleanupJob, Outcome};
use crate::config::LobaConfig;
use crate::dictionary::Dictionary;
use crate::ollama::OllamaClient;
use crate::prompt::PromptContext;
use crate::styles::Resolved;
use std::time::Duration;

pub struct Pipeline<'a> {
    pub client: &'a OllamaClient,
    pub config: &'a LobaConfig,
    pub dictionary: &'a Dictionary,
    pub style: &'a Resolved,
}

pub async fn run(p: Pipeline<'_>, raw: &str) -> Cleaned {
    let pre = p.dictionary.apply(raw);

    let skip = |why: String| Cleaned {
        text: pre.clone(),
        outcome: Outcome::Skipped(why),
        elapsed_ms: 0,
    };
    if !p.config.cleanup_enabled {
        return skip("cleanup off".into());
    }
    if !p.style.style.rewrite {
        let by = p.style.matched_by.as_deref().unwrap_or("default");
        return skip(format!("{} style for {by}", p.style.id));
    }

    let mut out = cleanup::clean(
        CleanupJob {
            client: p.client,
            model: &p.config.model,
            keep_alive: &p.config.keep_alive,
            budget: Duration::from_millis(p.config.timeout_ms),
            context: PromptContext {
                tone: Some(p.style.style.tone.clone()),
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
    use crate::styles::{AppContext, Styles};

    fn style_for(app: &str) -> Resolved {
        Styles::seed().resolve(&AppContext {
            name: Some(app.into()),
            ..Default::default()
        })
    }

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
                style: &style_for("Notes"),
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
                style: &style_for("Notes"),
            },
            "open loba",
        )
        .await;
        assert_eq!(out.text, "open L.O.B.A");
        assert!(matches!(out.outcome, Outcome::Skipped(_)));
    }

    #[tokio::test]
    async fn literal_destination_never_calls_the_llm() {
        // A server that would answer — but must not be asked.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hit = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hit2 = hit.clone();
        tokio::spawn(async move {
            if listener.accept().await.is_ok() {
                hit2.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let client = OllamaClient::new(&format!("http://{addr}"));
        let config = LobaConfig::default();
        let dictionary = Dictionary::seed();
        let out = run(
            Pipeline {
                client: &client,
                config: &config,
                dictionary: &dictionary,
                style: &style_for("Terminal"),
            },
            "git commit dash m um fix loba",
        )
        .await;
        assert_eq!(out.text, "git commit dash m um fix L.O.B.A");
        assert_eq!(
            out.outcome,
            Outcome::Skipped("literal style for Terminal".into())
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!hit.load(std::sync::atomic::Ordering::SeqCst));
    }
}
