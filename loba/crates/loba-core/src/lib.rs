//! L.O.B.A — the layer between transcription and paste.
//!
//! raw transcript ─▶ cleanup (local Ollama, hard budget) ─▶ paste
//!
//! The raw transcript is always the floor: every stage may improve the text,
//! none may withhold it.

pub mod cleanup;
pub mod config;
pub mod ollama;
pub mod prompt;

pub use cleanup::{clean, Cleaned, CleanupJob, Outcome};
pub use config::LobaConfig;
pub use ollama::OllamaClient;
