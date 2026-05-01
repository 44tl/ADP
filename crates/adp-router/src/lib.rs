//! ADP Router — LLM inference routing, model loading, and token management.
//!
//! Uses `llama.cpp` (via `llm` crate / rustformations) for local model inference.
//! No API keys, no cloud dependencies.
//!
//! # Architecture
//!
//! - [`model`] — Model loading and management.
//! - [`inference`] — Text generation and embedding inference.
//! - [`token_manager`] — Token counting and budget enforcement.
//! - [`router`] — [`InferenceRouter`] selects the appropriate model for a task.

pub mod error;
pub mod inference;
pub mod model;
pub mod router;
pub mod token_manager;

pub use error::{RouterError, Result};
pub use inference::{InferenceRequest, InferenceResult};
pub use model::{LoadedModel, ModelConfig};
pub use router::InferenceRouter;
pub use token_manager::TokenManager;
