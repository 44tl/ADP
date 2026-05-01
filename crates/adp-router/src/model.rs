//! Model loading and configuration.

use crate::error::{RouterError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, instrument};

/// Configuration for loading a local LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Human-readable model name.
    pub name: String,
    /// Path to the GGUF model file.
    pub model_path: PathBuf,
    /// Context size (in tokens).
    pub context_size: usize,
    /// Number of GPU layers to offload.
    pub gpu_layers: usize,
    /// Number of threads for CPU inference.
    pub threads: usize,
    /// Batch size for prompt processing.
    pub batch_size: usize,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            model_path: PathBuf::from("models/default.gguf"),
            context_size: 4096,
            gpu_layers: 0,
            threads: num_cpus::get(),
            batch_size: 512,
        }
    }
}

/// A loaded model ready for inference.
///
/// In production, this wraps the `llm` crate's model handle.
/// For now, it's a placeholder with the config.
#[derive(Debug, Clone)]
pub struct LoadedModel {
    pub config: ModelConfig,
    pub loaded_at: chrono::DateTime<chrono::Utc>,
}

impl LoadedModel {
    /// Load a model from disk.
    #[instrument]
    pub fn load(config: ModelConfig) -> Result<Self> {
        if !config.model_path.exists() {
            return Err(RouterError::ModelLoad(format!(
                "model file not found: {}",
                config.model_path.display()
            )));
        }

        info!(
            model = %config.name,
            path = %config.model_path.display(),
            context = config.context_size,
            "model loaded"
        );

        Ok(Self {
            config,
            loaded_at: chrono::Utc::now(),
        })
    }

    /// Estimate tokens for a given text.
    pub fn estimate_tokens(&self, text: &str) -> usize {
        // Rough heuristic: ~4 chars per token for English text.
        (text.len() as f32 / 4.0).ceil() as usize
    }
}
