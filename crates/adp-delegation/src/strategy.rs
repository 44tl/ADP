//! Delegation strategy implementations.
//!
//! Each [`DelegationStrategy`] defines how a task is assigned to agents:
//!
//! | Strategy | Behavior |
//! |----------|----------|
//! | [`SingleStrategy`] | One agent, one task. |
//! | [`BroadcastStrategy`] | N agents race; first completion wins. |
//! | [`VoteStrategy`] | N agents vote; quorum required. |
//! | [`PipelineStrategy`] | Chain of stages; sequential execution. |
//! | [`MapReduceStrategy`] | Parallel map tasks; reduce aggregates. |

use crate::error::{DelegationError, Result};
use crate::registry::AgentRegistry;
use adp_core::task::{BroadcastStrategy, MapReduceStrategy, PipelineStrategy, Strategy, VoteStrategy};
use adp_core::task::Id;
use adp_runtime::capabilities::CapabilitySet;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

/// Context passed to strategy execution.
#[derive(Debug, Clone)]
pub struct StrategyContext {
    /// The task being delegated.
    pub task_id: Id,
    /// Required capabilities for this task.
    pub required_capabilities: CapabilitySet,
    /// Task-specific context (JSON).
    pub task_context: serde_json::Value,
}

/// Trait for delegation strategies.
#[async_trait]
pub trait DelegationStrategy: Send + Sync {
    /// Given a registry and task context, return the list of agent IDs to delegate to.
    ///
    /// For strategies that spawn subtasks (Pipeline, MapReduce), this returns the
    /// agents for the *first* stage. Subsequent stages are handled by the engine.
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>>;

    /// Returns `true` if this strategy requires subtask spawning.
    fn spawns_subtasks(&self) -> bool {
        false
    }

    /// For strategies with subtasks, determine the next stage's agents.
    /// Default implementation returns empty (no more stages).
    async fn next_stage_agents(
        &self,
        _registry: &AgentRegistry,
        _ctx: &StrategyContext,
        _previous_results: &[serde_json::Value],
    ) -> Result<Vec<Id>> {
        Ok(Vec::new())
    }
}

// ===================================================================
// SingleStrategy
// ===================================================================

/// Assign to exactly one agent.
pub struct SingleStrategy;

#[async_trait]
impl DelegationStrategy for SingleStrategy {
    #[instrument(skip(registry, ctx), fields(task_id = %ctx.task_id))]
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>> {
        let agent = registry
            .find_one(&ctx.required_capabilities)
            .await
            .ok_or_else(|| {
                DelegationError::NoMatchingAgent(format!(
                    "no agent found for task {}",
                    ctx.task_id
                ))
            })?;

        info!(task_id = %ctx.task_id, agent_id = %agent.agent_id, "single delegation");
        Ok(vec![agent.agent_id])
    }
}

// ===================================================================
// BroadcastStrategy
// ===================================================================

/// Assign to N agents; first completion wins, losers are cancelled.
pub struct BroadcastStrategy {
    pub max_agents: u32,
}

#[async_trait]
impl DelegationStrategy for BroadcastStrategy {
    #[instrument(skip(registry, ctx), fields(task_id = %ctx.task_id, max_agents = self.max_agents))]
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>> {
        let mut matching = registry.find_matching(&ctx.required_capabilities).await;
        matching.sort_by(|a, b| b.last_heartbeat.cmp(&a.last_heartbeat)); // freshest first

        let selected: Vec<Id> = matching
            .into_iter()
            .take(self.max_agents as usize)
            .map(|e| e.agent_id)
            .collect();

        if selected.is_empty() {
            return Err(DelegationError::NoMatchingAgent(format!(
                "no agents found for broadcast task {}",
                ctx.task_id
            )));
        }

        info!(
            task_id = %ctx.task_id,
            agent_count = selected.len(),
            "broadcast delegation"
        );
        Ok(selected)
    }
}

// ===================================================================
// VoteStrategy
// ===================================================================

/// Assign to N agents; require quorum agreement (default 67%).
pub struct VoteStrategy {
    pub quorum_percent: u32,
    pub max_agents: u32,
}

impl Default for VoteStrategy {
    fn default() -> Self {
        Self {
            quorum_percent: 67,
            max_agents: 3,
        }
    }
}

#[async_trait]
impl DelegationStrategy for VoteStrategy {
    #[instrument(skip(registry, ctx), fields(task_id = %ctx.task_id, quorum = self.quorum_percent))]
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>> {
        let mut matching = registry.find_matching(&ctx.required_capabilities).await;
        matching.sort_by(|a, b| b.last_heartbeat.cmp(&a.last_heartbeat));

        let selected: Vec<Id> = matching
            .into_iter()
            .take(self.max_agents as usize)
            .map(|e| e.agent_id)
            .collect();

        if selected.is_empty() {
            return Err(DelegationError::NoMatchingAgent(format!(
                "no agents found for vote task {}",
                ctx.task_id
            )));
        }

        info!(
            task_id = %ctx.task_id,
            agent_count = selected.len(),
            quorum = self.quorum_percent,
            "vote delegation"
        );
        Ok(selected)
    }
}

// ===================================================================
// PipelineStrategy
// ===================================================================

/// Chain of stages; output of stage N is context input for stage N+1.
pub struct PipelineStrategy {
    pub stages: Vec<String>,
    pub current_stage: usize,
}

impl PipelineStrategy {
    pub fn new(stages: Vec<String>) -> Self {
        Self {
            stages,
            current_stage: 0,
        }
    }
}

#[async_trait]
impl DelegationStrategy for PipelineStrategy {
    fn spawns_subtasks(&self) -> bool {
        true
    }

    #[instrument(skip(registry, ctx), fields(task_id = %ctx.task_id, stage = self.current_stage))]
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>> {
        if self.current_stage >= self.stages.len() {
            return Ok(Vec::new());
        }

        let agent = registry
            .find_one(&ctx.required_capabilities)
            .await
            .ok_or_else(|| {
                DelegationError::NoMatchingAgent(format!(
                    "no agent found for pipeline stage {} of task {}",
                    self.current_stage, ctx.task_id
                ))
            })?;

        info!(
            task_id = %ctx.task_id,
            stage = self.current_stage,
            stage_name = %self.stages[self.current_stage],
            agent_id = %agent.agent_id,
            "pipeline delegation"
        );
        Ok(vec![agent.agent_id])
    }

    async fn next_stage_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
        _previous_results: &[serde_json::Value],
    ) -> Result<Vec<Id>> {
        let next = Self {
            stages: self.stages.clone(),
            current_stage: self.current_stage + 1,
        };
        next.select_agents(registry, ctx).await
    }
}

// ===================================================================
// MapReduceStrategy
// ===================================================================

/// Spawn M map tasks; reduce task aggregates results.
pub struct MapReduceStrategy {
    pub map_count: u32,
}

#[async_trait]
impl DelegationStrategy for MapReduceStrategy {
    fn spawns_subtasks(&self) -> bool {
        true
    }

    #[instrument(skip(registry, ctx), fields(task_id = %ctx.task_id, map_count = self.map_count))]
    async fn select_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
    ) -> Result<Vec<Id>> {
        let mut matching = registry.find_matching(&ctx.required_capabilities).await;
        matching.sort_by(|a, b| b.last_heartbeat.cmp(&a.last_heartbeat));

        let selected: Vec<Id> = matching
            .into_iter()
            .take(self.map_count as usize)
            .map(|e| e.agent_id)
            .collect();

        if selected.len() < self.map_count as usize {
            warn!(
                task_id = %ctx.task_id,
                requested = self.map_count,
                available = selected.len(),
                "insufficient agents for map-reduce"
            );
        }

        if selected.is_empty() {
            return Err(DelegationError::NoMatchingAgent(format!(
                "no agents found for map-reduce task {}",
                ctx.task_id
            )));
        }

        info!(
            task_id = %ctx.task_id,
            map_count = selected.len(),
            "map-reduce delegation"
        );
        Ok(selected)
    }

    async fn next_stage_agents(
        &self,
        registry: &AgentRegistry,
        ctx: &StrategyContext,
        _previous_results: &[serde_json::Value],
    ) -> Result<Vec<Id>> {
        // Reduce stage: single agent
        let agent = registry
            .find_one(&ctx.required_capabilities)
            .await
            .ok_or_else(|| {
                DelegationError::NoMatchingAgent(format!(
                    "no agent found for reduce stage of task {}",
                    ctx.task_id
                ))
            })?;
        Ok(vec![agent.agent_id])
    }
}

// ===================================================================
// Strategy factory
// ===================================================================

/// Create a [`DelegationStrategy`] from an ADP [`Strategy`].
pub fn strategy_from_adp(strategy: &Strategy) -> Box<dyn DelegationStrategy> {
    match strategy {
        Strategy::Single(_) => Box::new(SingleStrategy),
        Strategy::Broadcast(cfg) => Box::new(BroadcastStrategy {
            max_agents: cfg.max_agents,
        }),
        Strategy::Vote(cfg) => Box::new(VoteStrategy {
            quorum_percent: cfg.quorum_percent,
            max_agents: cfg.max_agents,
        }),
        Strategy::Pipeline(cfg) => Box::new(PipelineStrategy::new(cfg.stages.clone())),
        Strategy::MapReduce(cfg) => Box::new(MapReduceStrategy {
            map_count: cfg.map_count,
        }),
    }
}
