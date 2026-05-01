//! Inference router — selects models and routes requests.
//!
//! [`InferenceRouter`] maintains a pool of loaded models and routes
//! inference requests to the appropriate one based on task requirements.

use crate::error::{RouterError, Result};
use crate::inference::{InferenceEngine, InferenceRequest, InferenceResult};
use crate::model::{LoadedModel, ModelConfig};
use crate::token_manager::{TokenBudget, TokenManager};
use adp_core::task::Id;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, instrument, warn};

/// Routes inference requests to loaded models.
#[derive(Debug, Clone)]
pub struct InferenceRouter {
    models: Arc<RwLock<HashMap<String, InferenceEngine>>>,
    token_manager: TokenManager,
    default_model: String,
}

impl InferenceRouter {
    /// Create a new router with the given token budget.
    pub fn new(budget: TokenBudget, default_model: impl Into<String>) -> Self {
        Self {
            models: Arc::new(RwLock::new(HashMap::new())),
            token_manager: TokenManager::new(budget),
            default_model: default_model.into(),
        }
    }

    /// Load a model and add it to the router.
    #[instrument(skip(self))]
    pub async fn load_model(&self, config: ModelConfig) -> Result<()> {
        let model = LoadedModel::load(config.clone())?;
        let engine = InferenceEngine::new(model);
        let mut models = self.models.write().await;
        models.insert(config.name.clone(), engine);
        info!(model = %config.name, "model loaded into router");
        Ok(())
    }

    /// Route an inference request to the appropriate model.
    #[instrument(skip(self, request), fields(task_id = %task_id))]
    pub async fn infer(
        &self,
        task_id: Id,
        request: InferenceRequest,
        model_name: Option<&str>,
    ) -> Result<InferenceResult> {
        let model_name = model_name.unwrap_or(&self.default_model);
        let models = self.models.read().await;
        let engine = models
            .get(model_name)
            .ok_or_else(|| RouterError::ModelNotFound(model_name.to_string()))?;

        let prompt_tokens = engine.model.estimate_tokens(&request.prompt);
        self.token_manager
            .check_budget(task_id, prompt_tokens, request.max_tokens)
            .await?;

        let result = engine.generate(request).await?;

        self.token_manager
            .record_usage(task_id, result.tokens_prompt, result.tokens_generated)
            .await;

        Ok(result)
    }

    /// Generate embeddings using the default model.
    #[instrument(skip(self, text), fields(task_id = %task_id))]
    pub async fn embed(&self, task_id: Id, text: &str, model_name: Option<&str>) -> Result<Vec<f32>> {
        let model_name = model_name.unwrap_or(&self.default_model);
        let models = self.models.read().await;
        let engine = models
            .get(model_name)
            .ok_or_else(|| RouterError::ModelNotFound(model_name.to_string()))?;

        engine.embed(text).await
    }

    /// Get current token usage for a task.
    pub async fn token_usage(&self, task_id: &Id) -> crate::token_manager::TokenUsage {
        self.token_manager.get_usage(task_id).await
    }

    /// List loaded models.
    pub async fn list_models(&self) -> Vec<String> {
        let models = self.models.read().await;
        models.keys().cloned().collect()
    }
}
