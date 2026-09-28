//! L.O.B.A — the layer between transcription and paste.
//!
//! raw transcript ─▶ dictionary ─▶ cleanup (local Ollama, hard budget)
//!                ─▶ dictionary ─▶ paste
//!
//! The raw transcript is always the floor: every stage may improve the text,
//! none may withhold it.

pub mod cleanup;
pub mod config;
pub mod dictionary;
pub mod ollama;
pub mod pipeline;
pub mod prompt;

pub use cleanup::{clean, Cleaned, CleanupJob, Outcome};
pub use config::LobaConfig;
pub use dictionary::Dictionary;
pub use ollama::OllamaClient;
