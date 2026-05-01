//! DAG scheduler — orchestrates task lifecycle, parent-child blocking, and strategy rules.
//!
//! [`DagScheduler`] is the central coordinator. It decides when tasks transition,
//! when subtasks are spawned, and when results are aggregated. It is strictly
//! async and store-backed: every mutation is persisted before returning.
//!
//! # Concurrency
//!
//! The scheduler holds `Arc` references to the stores, so it is cheap to clone.
//! However, it does not provide transactional isolation across multiple store
//! calls — that is the responsibility of the store implementation (e.g., libSQL
//! transactions in production).

use crate::error::{AdpError, Result};
use crate::state_machine::{TaskStateMachine, TransitionCommand};
use crate::store::{EventStore, TaskStore};
use crate::task::{Event, Id, Task, TaskState};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

/// Central orchestrator for task DAGs.
#[derive(Debug)]
pub struct DagScheduler<S: TaskStore, E: EventStore> {
    state_machine: TaskStateMachine,
    task_store: Arc<S>,
    event_store: Arc<E>,
}

/// Outcomes from processing an external event through the scheduler.
#[derive(Debug, Clone, PartialEq)]
pub enum SchedulerOutcome {
    /// Task transitioned to a new state.
    Transitioned { task_id: Id, new_state: TaskState },
    /// Task spawned subtasks.
    SubtasksSpawned { parent_id: Id, subtask_ids: Vec<Id> },
    /// Parent task was unblocked because all children are terminal.
    ParentUnblocked { parent_id: Id },
    /// Nothing happened (idempotent or no-op).
    NoOp,
}

impl<S: TaskStore, E: EventStore> DagScheduler<S, E> {
    /// Create a new scheduler backed by the given stores.
    pub fn new(task_store: S, event_store: E) -> Self {
        Self {
            state_machine: TaskStateMachine::new(),
            task_store: Arc::new(task_store),
            event_store: Arc::new(event_store),
        }
    }

    // ------------------------------------------------------------------
    // Task lifecycle API
    // ------------------------------------------------------------------

    /// Persist a newly created task.
    #[instrument(skip(self, task), fields(task_id = %task.id))]
    pub async fn create_task(&self, task: &Task) -> Result<()> {
        self.task_store.insert(task).await?;
        info!(task_id = %task.id, task_type = %task.task_type, "task created");
        Ok(())
    }

    /// Delegate a pending task to an agent.
    #[instrument(skip(self), fields(task_id = %task_id, agent_id = %agent_id))]
    pub async fn delegate_task(&self, task_id: Id, agent_id: Id) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::Delegate { agent_id })?;
        self.persist_transition(&task, &events).await?;

        info!(task_id = %task_id, agent_id = %agent_id, "task delegated");
        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Mark a delegated task as executing.
    #[instrument(skip(self), fields(task_id = %task_id))]
    pub async fn start_execution(&self, task_id: Id) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::StartExecution)?;
        self.persist_transition(&task, &events).await?;

        info!(task_id = %task_id, "execution started");
        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Complete a leaf task (no subtasks).
    #[instrument(skip(self, result), fields(task_id = %task_id))]
    pub async fn complete_task(
        &self,
        task_id: Id,
        result: serde_json::Value,
    ) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::Complete { result })?;
        self.persist_transition(&task, &events).await?;

        // Check if this unblocks a parent.
        let parent_outcome = self.check_parent_unblocked(task.parent_id).await?;

        info!(task_id = %task_id, "task completed");

        if let Some(parent_id) = task.parent_id {
            if parent_outcome {
                return Ok(SchedulerOutcome::ParentUnblocked { parent_id });
            }
        }

        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Fail a task.
    ///
    /// Leaf tasks with remaining retries are automatically retried.
    /// Parent tasks (those with subtasks) do **not** auto-retry; they remain
    /// FAILED until explicitly retried by higher-level orchestration.
    #[instrument(skip(self, reason), fields(task_id = %task_id))]
    pub async fn fail_task(&self, task_id: Id, reason: String) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::Fail { reason: reason.clone() })?;
        self.persist_transition(&task, &events).await?;

        error!(task_id = %task_id, reason = %reason, "task failed");

        // Auto-retry leaf tasks only.
        if task.can_retry() && task.subtask_ids.is_empty() {
            debug!(
                task_id = %task_id,
                retry_count = task.retry_count,
                "auto-retrying leaf task"
            );
            let retry_events = self.state_machine.apply(&mut task, TransitionCommand::Retry)?;
            self.persist_transition(&task, &retry_events).await?;
            return Ok(SchedulerOutcome::Transitioned {
                task_id,
                new_state: task.state,
            });
        }

        // Notify parent that a subtask failed.
        if let Some(parent_id) = task.parent_id {
            self.check_parent_unblocked(Some(parent_id)).await?;
        }

        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Explicitly retry a failed task.
    #[instrument(skip(self), fields(task_id = %task_id))]
    pub async fn retry_task(&self, task_id: Id) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::Retry)?;
        self.persist_transition(&task, &events).await?;

        info!(
            task_id = %task_id,
            retry_count = task.retry_count,
            "task manually retried"
        );
        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Spawn subtasks for a parent task.
    ///
    /// The parent must be in `EXECUTING`. After this call the parent moves to
    /// `AWAITING_SUBTASKS`.
    #[instrument(skip(self, subtasks), fields(task_id = %parent_id, count = subtasks.len()))]
    pub async fn spawn_subtasks(
        &self,
        parent_id: Id,
        subtasks: Vec<Task>,
    ) -> Result<SchedulerOutcome> {
        if subtasks.is_empty() {
            warn!(parent_id = %parent_id, "spawn_subtasks called with empty list");
            // Treat as immediate completion.
            return self.complete_task(parent_id, serde_json::Value::Null).await;
        }

        let mut parent = self
            .task_store
            .get(&parent_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(parent_id.to_string()))?;

        let subtask_ids: Vec<Id> = subtasks.iter().map(|t| t.id).collect();

        // Insert subtasks first so they exist when the parent references them.
        for subtask in &subtasks {
            self.task_store.insert(subtask).await?;
        }

        let events = self.state_machine.apply(
            &mut parent,
            TransitionCommand::SpawnSubtasks {
                subtask_ids: subtask_ids.clone(),
            },
        )?;
        self.persist_transition(&parent, &events).await?;

        info!(
            parent_id = %parent_id,
            subtask_count = subtasks.len(),
            "subtasks spawned"
        );

        Ok(SchedulerOutcome::SubtasksSpawned {
            parent_id,
            subtask_ids,
        })
    }

    /// Report that all subtasks of a task have completed.
    ///
    /// Verifies that every subtask is in a terminal state before transitioning
    /// the parent to `AGGREGATING`.
    #[instrument(skip(self), fields(task_id = %task_id))]
    pub async fn subtasks_completed(&self, task_id: Id) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        // Verify all subtasks are terminal.
        for subtask_id in &task.subtask_ids {
            let subtask = self
                .task_store
                .get(subtask_id)
                .await?
                .ok_or_else(|| AdpError::TaskNotFound(subtask_id.to_string()))?;
            if !subtask.is_terminal() {
                return Err(AdpError::DelegationFailed(format!(
                    "subtask {} is not terminal (state: {})",
                    subtask_id, subtask.state
                )));
            }
        }

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::SubtasksCompleted)?;
        self.persist_transition(&task, &events).await?;

        info!(
            task_id = %task_id,
            "all subtasks completed, moving to aggregating"
        );
        Ok(SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        })
    }

    /// Aggregate subtask results and complete the parent task.
    #[instrument(skip(self, result), fields(task_id = %task_id))]
    pub async fn aggregate_and_complete(
        &self,
        task_id: Id,
        result: serde_json::Value,
    ) -> Result<SchedulerOutcome> {
        let mut task = self
            .task_store
            .get(&task_id)
            .await?
            .ok_or_else(|| AdpError::TaskNotFound(task_id.to_string()))?;

        let events = self
            .state_machine
            .apply(&mut task, TransitionCommand::Aggregate { result })?;
        self.persist_transition(&task, &events).await?;

        let parent_id = task.parent_id;
        let outcome = SchedulerOutcome::Transitioned {
            task_id,
            new_state: task.state,
        };

        // Propagate unblocking up the tree.
        if let Some(pid) = parent_id {
            if self.check_parent_unblocked(Some(pid)).await? {
                return Ok(SchedulerOutcome::ParentUnblocked { parent_id: pid });
            }
        }

        Ok(outcome)
    }

    /// Cancel a task and all its non-terminal descendants (DFS).
    ///
    /// Used by BROADCAST strategy to cancel loser agents.
    #[instrument(skip(self, reason), fields(task_id = %task_id))]
    pub async fn cancel_task(&self, task_id: Id, reason: String) -> Result<Vec<SchedulerOutcome>> {
        let mut outcomes = Vec::new();
        let mut to_cancel = vec![task_id];
        let mut visited = HashSet::new();

        while let Some(id) = to_cancel.pop() {
            if !visited.insert(id) {
                continue;
            }

            let mut task = match self.task_store.get(&id).await? {
                Some(t) => t,
                None => {
                    warn!(task_id = %id, "task not found during cancel");
                    continue;
                }
            };

            if task.is_terminal() {
                continue;
            }

            // Queue children for cancellation first (DFS).
            for subtask_id in task.subtask_ids.clone() {
                to_cancel.push(subtask_id);
            }

            let events = self.state_machine.apply(
                &mut task,
                TransitionCommand::Cancel {
                    reason: reason.clone(),
                },
            )?;
            self.persist_transition(&task, &events).await?;

            outcomes.push(SchedulerOutcome::Transitioned {
                task_id: id,
                new_state: task.state,
            });
            info!(task_id = %id, reason = %reason, "task cancelled");
        }

        Ok(outcomes)
    }

    // ------------------------------------------------------------------
    // Queries
    // ------------------------------------------------------------------

    /// Retrieve the entire DAG rooted at `root_id`.
    pub async fn get_dag(&self, root_id: Id) -> Result<HashMap<Id, Task>> {
        let mut result = HashMap::new();
        let mut queue = VecDeque::from([root_id]);

        while let Some(id) = queue.pop_front() {
            if result.contains_key(&id) {
                continue;
            }
            if let Some(task) = self.task_store.get(&id).await? {
                for subtask_id in &task.subtask_ids {
                    queue.push_back(*subtask_id);
                }
                result.insert(id, task);
            }
        }

        Ok(result)
    }

    /// Find all tasks ready for delegation.
    ///
    /// A task is delegatable if it is `PENDING` and either:
    /// - it has no parent, or
    /// - its parent has already spawned it (i.e., parent is `AWAITING_SUBTASKS` or later).
    pub async fn find_delegatable_tasks(&self) -> Result<Vec<Task>> {
        let all_tasks = self.task_store.list_all().await?;
        let mut ready = Vec::new();

        for task in all_tasks {
            if task.state != TaskState::Pending {
                continue;
            }

            if let Some(parent_id) = task.parent_id {
                let parent = self.task_store.get(&parent_id).await?;
                if let Some(p) = parent {
                    // Parent must have already spawned this task.
                    if matches!(
                        p.state,
                        TaskState::Pending
                            | TaskState::Delegated
                            | TaskState::Executing
                            | TaskState::Failed
                            | TaskState::Cancelled
                    ) {
                        continue;
                    }
                }
            }

            ready.push(task);
        }

        Ok(ready)
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Check whether a parent task's children are all terminal.
    ///
    /// If so, transitions the parent appropriately:
    /// - Any child failed → parent fails.
    /// - All children cancelled → parent cancels.
    /// - Otherwise → parent moves to `AGGREGATING`.
    #[instrument(skip(self), fields(parent_id = ?parent_id))]
    async fn check_parent_unblocked(&self, parent_id: Option<Id>) -> Result<bool> {
        let parent_id = match parent_id {
            Some(id) => id,
            None => return Ok(false),
        };

        let parent = match self.task_store.get(&parent_id).await? {
            Some(t) => t,
            None => return Ok(false),
        };

        if parent.state != TaskState::AwaitingSubtasks {
            return Ok(false);
        }

        // Collect subtask terminal statuses.
        let mut subtask_states = Vec::with_capacity(parent.subtask_ids.len());
        for subtask_id in &parent.subtask_ids {
            if let Some(subtask) = self.task_store.get(subtask_id).await? {
                subtask_states.push((subtask.id, subtask.state, subtask.is_terminal()));
            }
        }

        let all_terminal = subtask_states.iter().all(|(_, _, terminal)| *terminal);
        if !all_terminal {
            return Ok(false);
        }

        let any_failed = subtask_states
            .iter()
            .any(|(_, state, _)| *state == TaskState::Failed);
        let any_cancelled = subtask_states
            .iter()
            .any(|(_, state, _)| *state == TaskState::Cancelled);

        if any_failed {
            self.fail_task(parent_id, "subtask failed".to_string())
                .await?;
            return Ok(true);
        }

        if any_cancelled {
            let all_cancelled = subtask_states
                .iter()
                .all(|(_, state, _)| *state == TaskState::Cancelled);
            if all_cancelled {
                self.cancel_task(parent_id, "all subtasks cancelled".to_string())
                    .await?;
                return Ok(true);
            }
            // Partial cancellation — wait for remaining subtasks.
            return Ok(false);
        }

        // All succeeded → move to aggregating.
        self.subtasks_completed(parent_id).await?;
        Ok(true)
    }

    /// Persist task update and associated events.
    ///
    /// TODO: In production with libSQL, wrap this in a transaction.
    async fn persist_transition(&self, task: &Task, events: &[Event]) -> Result<()> {
        self.task_store.update(task).await?;
        for event in events {
            self.event_store.append(event).await?;
        }
        Ok(())
    }
}
