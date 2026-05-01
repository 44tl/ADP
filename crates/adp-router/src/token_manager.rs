//! Token budget management.
//!
//! [`TokenManager`] tracks token consumption across agents and tasks,
//! enforcing per-task and global budgets.

use crate::error::{RouterError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};
use adp_core::task::Id;

/// Token budget configuration.
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    pub max_prompt_tokens: usize,
    pub max_completion_tokens: usize,
    pub max_total_tokens: usize,
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self {
            max_prompt_tokens: 2048,
            max_completion_tokens: 1024,
            max_total_tokens: 4096,
        }
    }
}

/// Tracks token usage.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

impl TokenUsage {
    pub fn total(&self) -> usize {
        self.prompt_tokens + self.completion_tokens
    }
}

/// Manages token budgets for tasks and agents.
#[derive(Debug, Clone)]
pub struct TokenManager {
    budget: TokenBudget,
    usage: Arc<RwLock<HashMap<Id, TokenUsage>>>,
}

impl TokenManager {
    pub fn new(budget: TokenBudget) -> Self {
        Self {
            budget,
            usage: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a request would exceed the budget.
    pub async fn check_budget(&self, task_id: Id, prompt_tokens: usize, completion_tokens: usize) -> Result<()> {
        let usage = self.usage.read().await;
        let current = usage.get(&task_id).cloned().unwrap_or_default();
        drop(usage);

        let new_total = current.total() + prompt_tokens + completion_tokens;

        if prompt_tokens > self.budget.max_prompt_tokens {
            return Err(RouterError::TokenBudgetExceeded(format!(
                "prompt tokens {} > max {}",
                prompt_tokens, self.budget.max_prompt_tokens
            )));
        }

        if completion_tokens > self.budget.max_completion_tokens {
            return Err(RouterError::TokenBudgetExceeded(format!(
                "completion tokens {} > max {}",
                completion_tokens, self.budget.max_completion_tokens
            )));
        }

        if new_total > self.budget.max_total_tokens {
            return Err(RouterError::TokenBudgetExceeded(format!(
                "total tokens {} > max {}",
                new_total, self.budget.max_total_tokens
            )));
        }

        Ok(())
    }

    /// Record token usage for a task.
    pub async fn record_usage(&self, task_id: Id, prompt_tokens: usize, completion_tokens: usize) {
        let mut usage = self.usage.write().await;
        usage
            .entry(task_id)
            .and_modify(|u| {
                u.prompt_tokens += prompt_tokens;
                u.completion_tokens += completion_tokens;
            })
            .or_insert(TokenUsage {
                prompt_tokens,
                completion_tokens,
            });

        let total = usage.get(&task_id).unwrap().total();
        debug!(task_id = %task_id, total_tokens = total, "token usage recorded");
    }

    /// Get current usage for a task.
    pub async fn get_usage(&self, task_id: &Id) -> TokenUsage {
        let usage = self.usage.read().await;
        usage.get(task_id).cloned().unwrap_or_default()
    }
}
