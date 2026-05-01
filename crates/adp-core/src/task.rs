//! Core domain types for ADP tasks, agents, and events.
//!
//! These types mirror the Protobuf schema in `proto/adp/core/v1/` but are
//! native Rust structs with serde support for JSON serialization.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Unique identifier used for tasks, agents, delegations, and events.
///
/// Wraps a [`Ulid`] for lexicographically sortable, 128-bit identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(pub Ulid);

impl Id {
    /// Generate a new random ULID.
    pub fn new() -> Self {
        Self(Ulid::new())
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Ulid> for Id {
    fn from(u: Ulid) -> Self {
        Self(u)
    }
}

/// The atomic unit of work in ADP.
///
/// A task's *identity* (id, parent_id, task_type, strategy, created_at) is immutable.
/// Its *runtime state* is managed exclusively by the [`TaskStateMachine`](crate::state_machine::TaskStateMachine).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Id,
    pub parent_id: Option<Id>,
    pub state: TaskState,
    pub task_type: String,
    /// Opaque JSON context. Agents may append; they never mutate upstream context.
    pub context: serde_json::Value,
    pub strategy: Strategy,
    pub retry_count: u32,
    pub max_retries: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub subtask_ids: Vec<Id>,
    pub agent_id: Option<Id>,
}

impl Task {
    /// Start building a new task.
    pub fn builder(task_type: impl Into<String>) -> TaskBuilder {
        TaskBuilder::new(task_type)
    }

    /// Returns `true` if the task is in a terminal state.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.state,
            TaskState::Completed | TaskState::Failed | TaskState::Cancelled
        )
    }

    /// Returns `true` if the task has failed and has remaining retry attempts.
    pub fn can_retry(&self) -> bool {
        self.state == TaskState::Failed && self.retry_count < self.max_retries
    }
}

/// Builder for [`Task`].
#[derive(Debug)]
pub struct TaskBuilder {
    task_type: String,
    parent_id: Option<Id>,
    context: serde_json::Value,
    strategy: Strategy,
    max_retries: u32,
}

impl TaskBuilder {
    fn new(task_type: impl Into<String>) -> Self {
        Self {
            task_type: task_type.into(),
            parent_id: None,
            context: serde_json::Value::Null,
            strategy: Strategy::Single(SingleStrategy),
            max_retries: 3,
        }
    }

    /// Set the parent task id.
    pub fn parent_id(mut self, id: Id) -> Self {
        self.parent_id = Some(id);
        self
    }

    /// Set the initial JSON context.
    pub fn context(mut self, ctx: serde_json::Value) -> Self {
        self.context = ctx;
        self
    }

    /// Set the delegation strategy.
    pub fn strategy(mut self, strategy: Strategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Set the maximum number of retry attempts.
    pub fn max_retries(mut self, max: u32) -> Self {
        self.max_retries = max;
        self
    }

    /// Build the [`Task`]. Sets `created_at` and `updated_at` to now.
    pub fn build(self) -> Task {
        let now = Utc::now();
        Task {
            id: Id::new(),
            parent_id: self.parent_id,
            state: TaskState::Pending,
            task_type: self.task_type,
            context: self.context,
            strategy: self.strategy,
            retry_count: 0,
            max_retries: self.max_retries,
            created_at: now,
            updated_at: now,
            completed_at: None,
            subtask_ids: Vec::new(),
            agent_id: None,
        }
    }
}

/// States in the task lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaskState {
    Pending,
    Delegated,
    Executing,
    AwaitingSubtasks,
    Aggregating,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskState::Pending => write!(f, "PENDING"),
            TaskState::Delegated => write!(f, "DELEGATED"),
            TaskState::Executing => write!(f, "EXECUTING"),
            TaskState::AwaitingSubtasks => write!(f, "AWAITING_SUBTASKS"),
            TaskState::Aggregating => write!(f, "AGGREGATING"),
            TaskState::Completed => write!(f, "COMPLETED"),
            TaskState::Failed => write!(f, "FAILED"),
            TaskState::Cancelled => write!(f, "CANCELLED"),
        }
    }
}

/// Delegation strategy for a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum Strategy {
    /// Assign to exactly one agent.
    Single(SingleStrategy),
    /// Assign to N agents; first completion wins, losers are cancelled.
    Broadcast(BroadcastStrategy),
    /// Assign to N agents; require quorum (default 2/3) agreement.
    Vote(VoteStrategy),
    /// Chain of stages; output of stage N is context input for stage N+1.
    Pipeline(PipelineStrategy),
    /// Spawn M map tasks; reduce task aggregates results.
    MapReduce(MapReduceStrategy),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SingleStrategy;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BroadcastStrategy {
    pub max_agents: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoteStrategy {
    /// Quorum as a percentage (0–100). Default 67.
    pub quorum_percent: u32,
    pub max_agents: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineStrategy {
    pub stages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapReduceStrategy {
    pub map_count: u32,
}

/// An immutable audit event recording something that happened to a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: Id,
    pub task_id: Id,
    pub event_type: EventType,
    pub from_state: Option<TaskState>,
    pub to_state: Option<TaskState>,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Classification of audit events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    StateTransition,
    SubtaskCreated,
    DelegationCreated,
    DelegationCompleted,
    RetryTriggered,
    Cancelled,
}
