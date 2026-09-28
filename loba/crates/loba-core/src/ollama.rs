//! Minimal Ollama client: `/api/chat` (non-streaming), `/api/tags`, and a
//! preload call. Nothing here retries — the caller owns the time budget.

use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }
}

#[derive(Debug)]
pub enum OllamaError {
    Timeout,
    Unreachable(String),
    Status(u16, String),
    Decode(String),
}

impl std::fmt::Display for OllamaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timed out"),
            Self::Unreachable(e) => write!(f, "Ollama unreachable: {e}"),
            Self::Status(code, body) => write!(f, "Ollama returned {code}: {body}"),
            Self::Decode(e) => write!(f, "unexpected Ollama response: {e}"),
        }
    }
}

impl std::error::Error for OllamaError {}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    stream: bool,
    /// Qwen3 is a hybrid reasoning model; thinking would spend the whole
    /// budget before the first visible token. Older servers ignore the field.
    think: bool,
    keep_alive: &'a str,
    options: ChatOptions,
}

#[derive(Serialize)]
struct ChatOptions {
    temperature: f32,
    num_predict: i32,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: Option<Message>,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Deserialize)]
struct TagModel {
    name: String,
}

#[derive(Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    base_url: String,
}

impl OllamaClient {
    pub fn new(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            // Loopback only: a system proxy must never see a transcript.
            .no_proxy()
            .connect_timeout(Duration::from_millis(400))
            .build()
            .expect("reqwest client with static config");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// One chat round trip, bounded by `budget` end to end (connect, queue,
    /// generation, body read).
    pub async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        keep_alive: &str,
        num_predict: i32,
        budget: Duration,
    ) -> Result<String, OllamaError> {
        let body = ChatRequest {
            model,
            messages,
            stream: false,
            think: false,
            keep_alive,
            options: ChatOptions {
                temperature: 0.2,
                num_predict,
            },
        };
        let url = format!("{}/api/chat", self.base_url);
        let call = async {
            let resp = self
                .http
                .post(&url)
                .json(&body)
                .send()
                .await
                .map_err(|e| OllamaError::Unreachable(e.to_string()))?;
            let status = resp.status();
            if !status.is_success() {
                let text = resp.text().await.unwrap_or_default();
                return Err(OllamaError::Status(status.as_u16(), truncate(&text, 200)));
            }
            let parsed: ChatResponse = resp
                .json()
                .await
                .map_err(|e| OllamaError::Decode(e.to_string()))?;
            Ok(parsed.message.map(|m| m.content).unwrap_or_default())
        };
        match tokio::time::timeout(budget, call).await {
            Ok(r) => r,
            Err(_) => Err(OllamaError::Timeout),
        }
    }

    /// Load `model` into memory without generating. Called when the user starts
    /// speaking so the weights are hot by the time the transcript lands.
    pub async fn preload(&self, model: &str, keep_alive: &str) -> Result<(), OllamaError> {
        let url = format!("{}/api/chat", self.base_url);
        let body = serde_json::json!({ "model": model, "messages": [], "keep_alive": keep_alive });
        let call = self.http.post(&url).json(&body).send();
        match tokio::time::timeout(Duration::from_secs(60), call).await {
            Ok(Ok(r)) if r.status().is_success() => Ok(()),
            Ok(Ok(r)) => Err(OllamaError::Status(r.status().as_u16(), String::new())),
            Ok(Err(e)) => Err(OllamaError::Unreachable(e.to_string())),
            Err(_) => Err(OllamaError::Timeout),
        }
    }

    /// Installed model names, for the picker. Short timeout: this backs a UI.
    pub async fn list_models(&self) -> Result<Vec<String>, OllamaError> {
        let url = format!("{}/api/tags", self.base_url);
        let call = async {
            let resp = self
                .http
                .get(&url)
                .send()
                .await
                .map_err(|e| OllamaError::Unreachable(e.to_string()))?;
            if !resp.status().is_success() {
                return Err(OllamaError::Status(resp.status().as_u16(), String::new()));
            }
            let tags: TagsResponse = resp
                .json()
                .await
                .map_err(|e| OllamaError::Decode(e.to_string()))?;
            Ok(tags.models.into_iter().map(|m| m.name).collect())
        };
        tokio::time::timeout(Duration::from_millis(1500), call)
            .await
            .unwrap_or(Err(OllamaError::Timeout))
    }
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
