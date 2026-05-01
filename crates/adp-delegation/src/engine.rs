//! Delegation engine — ties together strategies, registry, and consensus.
//!
//! [`DelegationEngine`] is the high-level interface used by the scheduler
//! (or external orchestrators) to delegate tasks. It:
//!
//! 1. Looks up the task's [`Strategy`](adp_core::task::Strategy).
//! 2. Uses the appropriate [`DelegationStrategy`](crate::strategy::DelegationStrategy)
//!    to select agent(s).
//! 3. For [`VOTE`](adp_core::task::Strategy::Vote), coordinates consensus.
//! 4. For [`BROADCAST`](adp_core::task::Strategy::Broadcast), handles winner/loser logic.
//! 5. For [`PIPELINE`](adp_core::task::Strategy::Pipeline) and
//!    [`MAP_REDUCE`](adp_core::task::Strategy::MapReduce), manages subtask spawning.

use crate::consensus::{ConsensusEngine, ConsensusOutcome};
use crate::error::{DelegationError, Result};
use crate::registry::AgentRegistry;
use crate::strategy::{strategy_from_adp, DelegationStrategy, StrategyContext};
use adp_core::scheduler::DagScheduler;
use adp_core::store::{EventStore, TaskStore};
use adp_core::task::{Id, Strategy, Task, TaskState};
use adp_runtime::capabilities::CapabilitySet;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};

/// High-level delegation coordinator.
#[derive(Debug, Clone)]
pub struct DelegationEngine {
    registry: AgentRegistry,
    consensus: ConsensusEngine,
    /// Tracks in-flight broadcast tasks: task_id -> (winner_agent_id, loser_agent_ids).
    broadcast_tracker: Arc<RwLock<HashMap<Id, (Id, Vec<Id>)>>>,
    /// Tracks in-flight vote tasks: task_id -> (quorum_percent, results_so_far).
    vote_tracker: Arc<RwLock<HashMap<Id, VoteTracker>>>,
}

#[derive(Debug, Clone)]
struct VoteTracker {
    quorum_percent: u32,
    results: Vec<(Id, Option<serde_json::Value>)>,
    total_agents: usize,
}

impl DelegationEngine {
    /// Create a new delegation engine.
    pub fn new(registry: AgentRegistry) -> Self {
        Self {
            registry,
            consensus: ConsensusEngine::new(),
            broadcast_tracker: Arc::new(RwLock::new(HashMap::new())),
            vote_tracker: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Delegate a task to appropriate agent(s) based on its strategy.
    ///
    /// Returns the list of agent IDs the task was delegated to.
    #[instrument(skip(self, task, scheduler), fields(task_id = %task.id))]
    pub async fn delegate<S: TaskStore, E: EventStore>(
        &self,
        task: &Task,
        scheduler: &DagScheduler<S, E>,
        required_capabilities: CapabilitySet,
    ) -> Result<Vec<Id>> {
        let ctx = StrategyContext {
            task_id: task.id,
            required_capabilities,
            task_context: task.context.clone(),
        };

        let strategy = strategy_from_adp(&task.strategy);
        let agent_ids = strategy.select_agents(&self.registry, &ctx).await?;

        if agent_ids.is_empty() {
            return Err(DelegationError::NoMatchingAgent(format!(
                "strategy returned no agents for task {}",
                task.id
            )));
        }

        // Handle strategy-specific setup.
        match &task.strategy {
            Strategy::Single(_) => {
                let agent_id = agent_ids[0];
                scheduler.delegate_task(task.id, agent_id).await?;
                info!(task_id = %task.id, agent_id = %agent_id, "single delegation complete");
            }
            Strategy::Broadcast(cfg) => {
                // Spawn subtasks for each agent.
                let subtasks: Vec<Task> = agent_ids
                    .iter()
                    .map(|agent_id| {
                        Task::builder("broadcast.run")
                            .parent_id(task.id)
                            .context(json!({ "agent_id": agent_id.to_string() }))
                            .build()
                    })
                    .collect();

                scheduler.spawn_subtasks(task.id, subtasks).await?;

                // Track for later cancellation.
                let mut tracker = self.broadcast_tracker.write().await;
                // Winner unknown at this point; will be set on first completion.
                tracker.insert(task.id, (Id::new(), agent_ids.clone()));

                info!(
                    task_id = %task.id,
                    agent_count = agent_ids.len(),
                    "broadcast delegation complete"
                );
            }
            Strategy::Vote(cfg) => {
                let subtasks: Vec<Task> = agent_ids
                    .iter()
                    .map(|agent_id| {
                        Task::builder("vote.run")
                            .parent_id(task.id)
                            .context(json!({ "agent_id": agent_id.to_string() }))
                            .build()
                    })
                    .collect();

                scheduler.spawn_subtasks(task.id, subtasks).await?;

                let mut tracker = self.vote_tracker.write().await;
                tracker.insert(
                    task.id,
                    VoteTracker {
                        quorum_percent: cfg.quorum_percent,
                        results: Vec::new(),
                        total_agents: agent_ids.len(),
                    },
                );

                info!(
                    task_id = %task.id,
                    agent_count = agent_ids.len(),
                    quorum = cfg.quorum_percent,
                    "vote delegation complete"
                );
            }
            Strategy::Pipeline(cfg) => {
                // First stage is delegated immediately.
                let agent_id = agent_ids[0];
                scheduler.delegate_task(task.id, agent_id).await?;
                info!(
                    task_id = %task.id,
                    stage = 0,
                    stage_name = %cfg.stages[0],
                    "pipeline delegation started"
                );
            }
            Strategy::MapReduce(cfg) => {
                // Map phase: spawn subtasks.
                let subtasks: Vec<Task> = agent_ids
                    .iter()
                    .map(|agent_id| {
                        Task::builder("mapreduce.map")
                            .parent_id(task.id)
                            .context(json!({ "agent_id": agent_id.to_string() }))
                            .build()
                    })
                    .collect();

                scheduler.spawn_subtasks(task.id, subtasks).await?;
                info!(
                    task_id = %task.id,
                    map_count = agent_ids.len(),
                    "map-reduce delegation started"
                );
            }
        }

        Ok(agent_ids)
    }

    /// Handle completion of a broadcast subtask.
    ///
    /// Cancels all other subtasks and marks the parent complete.
    #[instrument(skip(self, scheduler), fields(task_id = %parent_id, winner_id = %winner_agent_id))]
    pub async fn broadcast_winner<S: TaskStore, E: EventStore>(
        &self,
        parent_id: Id,
        winner_agent_id: Id,
        result: serde_json::Value,
        scheduler: &DagScheduler<S, E>,
    ) -> Result<()> {
        let tracker = self.broadcast_tracker.read().await;
        let (_, all_agents) = tracker
            .get(&parent_id)
            .ok_or_else(|| DelegationError::StrategyFailed("broadcast not tracked".to_string()))?;

        let losers: Vec<Id> = all_agents
            .iter()
            .filter(|&&id| id != winner_agent_id)
            .copied()
            .collect();
        drop(tracker);

        // Cancel loser subtasks.
        for loser_id in &losers {
            // Find the subtask for this loser agent.
            let dag = scheduler.get_dag(parent_id).await?;
            for (task_id, task) in dag {
                if task.parent_id == Some(parent_id) {
                    if let Some(ctx) = task.context.get("agent_id") {
                        if ctx.as_str() == Some(&loser_id.to_string()) {
                            scheduler
                                .cancel_task(task_id, "broadcast loser".to_string())
                                .await?;
                        }
                    }
                }
            }
        }

        // Complete the parent with the winner's result.
        scheduler.complete_task(parent_id, result).await?;

        // Clean up tracker.
        let mut tracker = self.broadcast_tracker.write().await;
        tracker.remove(&parent_id);

        info!(parent_id = %parent_id, "broadcast resolved");
        Ok(())
    }

    /// Record a vote result from an agent.
    ///
    /// When all agents have voted, evaluates consensus and either completes
    /// the parent or triggers re-delegation.
    #[instrument(skip(self, scheduler), fields(task_id = %parent_id, agent_id = %agent_id))]
    pub async fn record_vote<S: TaskStore, E: EventStore>(
        &self,
        parent_id: Id,
        agent_id: Id,
        result: Option<serde_json::Value>,
        scheduler: &DagScheduler<S, E>,
    ) -> Result<Option<serde_json::Value>> {
        let mut tracker = self.vote_tracker.write().await;
        let tracker_entry = tracker
            .get_mut(&parent_id)
            .ok_or_else(|| DelegationError::StrategyFailed("vote not tracked".to_string()))?;

        tracker_entry.results.push((agent_id, result));

        if tracker_entry.results.len() < tracker_entry.total_agents {
            debug!(
                received = tracker_entry.results.len(),
                total = tracker_entry.total_agents,
                "waiting for more votes"
            );
            return Ok(None); // Still waiting
        }

        // All votes in. Evaluate consensus.
        let quorum = tracker_entry.quorum_percent;
        let results = std::mem::take(&mut tracker_entry.results);
        drop(tracker);

        let outcome = self.consensus.evaluate(results, quorum)?;

        match outcome {
            ConsensusOutcome::Reached {
                result,
                agreeing_agents,
                total_agents,
            } => {
                info!(
                    parent_id = %parent_id,
                    agreeing = agreeing_agents.len(),
                    total = total_agents,
                    "vote consensus reached"
                );
                scheduler.complete_task(parent_id, result.clone()).await?;
                let mut tracker = self.vote_tracker.write().await;
                tracker.remove(&parent_id);
                Ok(Some(result))
            }
            ConsensusOutcome::Failed { reason, .. } => {
                warn!(parent_id = %parent_id, reason = %reason, "vote consensus failed");
                // Mark parent as failed; higher-level orchestration decides re-delegation.
                scheduler
                    .fail_task(parent_id, format!("consensus failed: {reason}"))
                    .await?;
                let mut tracker = self.vote_tracker.write().await;
                tracker.remove(&parent_id);
                Ok(None)
            }
        }
    }

    /// Advance a pipeline to the next stage.
    #[instrument(skip(self, scheduler), fields(task_id = %task_id, stage = stage_index))]
    pub async fn pipeline_next_stage<S: TaskStore, E: EventStore>(
        &self,
        task_id: Id,
        stage_index: usize,
        previous_result: serde_json::Value,
        scheduler: &DagScheduler<S, E>,
        required_capabilities: CapabilitySet,
    ) -> Result<Option<Id>> {
        let task = scheduler
            .get_dag(task_id)
            .await?
            .get(&task_id)
            .cloned()
            .ok_or_else(|| DelegationError::StrategyFailed("task not found".to_string()))?;

        let Strategy::Pipeline(cfg) = &task.strategy else {
            return Err(DelegationError::StrategyFailed(
                "not a pipeline task".to_string(),
            ));
        };

        if stage_index >= cfg.stages.len() {
            // All stages complete.
            scheduler.complete_task(task_id, previous_result).await?;
            return Ok(None);
        }

        let ctx = StrategyContext {
            task_id,
            required_capabilities,
            task_context: json!({
                "stage": stage_index,
                "stage_name": cfg.stages[stage_index],
                "previous_result": previous_result,
            }),
        };

        let strategy = crate::strategy::PipelineStrategy::new(cfg.stages.clone());
        // Hack: we need to advance current_stage. In production, this would be
        // stored in the task context or a separate table.
        let agent_ids = strategy.select_agents(&self.registry, &ctx).await?;

        if let Some(&agent_id) = agent_ids.first() {
            scheduler.delegate_task(task_id, agent_id).await?;
            info!(
                task_id = %task_id,
                stage = stage_index,
                agent_id = %agent_id,
                "pipeline stage delegated"
            );
            Ok(Some(agent_id))
        } else {
            Err(DelegationError::NoMatchingAgent(format!(
                "no agent for pipeline stage {} of task {}",
                stage_index, task_id
            )))
        }
    }
}
