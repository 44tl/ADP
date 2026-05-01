//! libSQL (Turso embedded) implementations of [`TaskStore`] and [`EventStore`].
//!
//! These stores persist tasks and events to a local SQLite-compatible database.
//! All queries use parameterized statements. JSON fields are serialized via
//! [`serde_json`].

use crate::error::{AdpError, Result};
use crate::store::{EventStore, TaskStore};
use crate::task::{Event, EventType, Id, Task, TaskState};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use libsql::params;
use std::sync::Arc;
use tracing::{debug, instrument};
use ulid::Ulid;

/// libSQL-backed task store.
#[derive(Debug, Clone)]
pub struct LibSqlTaskStore {
    conn: Arc<libsql::Connection>,
}

impl LibSqlTaskStore {
    /// Create a new store from an existing libSQL connection.
    pub fn new(conn: libsql::Connection) -> Self {
        Self {
            conn: Arc::new(conn),
        }
    }
}

/// libSQL-backed event store.
#[derive(Debug, Clone)]
pub struct LibSqlEventStore {
    conn: Arc<libsql::Connection>,
}

impl LibSqlEventStore {
    /// Create a new store from an existing libSQL connection.
    pub fn new(conn: libsql::Connection) -> Self {
        Self {
            conn: Arc::new(conn),
        }
    }
}

// ===================================================================
// TaskStore implementation
// ===================================================================

#[async_trait]
impl TaskStore for LibSqlTaskStore {
    #[instrument(skip(self, task), fields(task_id = %task.id))]
    async fn insert(&self, task: &Task) -> Result<()> {
        let sql = r#"
            INSERT INTO tasks (
                id, parent_id, state, task_type, context, strategy,
                retry_count, max_retries, created_at, updated_at,
                completed_at, subtask_ids, agent_id, deleted_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)
        "#;

        self.conn
            .execute(
                sql,
                params!(
                    task.id.to_string(),
                    task.parent_id.map(|id| id.to_string()),
                    task.state.to_string(),
                    &task.task_type,
                    serde_json::to_string(&task.context)
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    serde_json::to_string(&task.strategy)
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    task.retry_count as i64,
                    task.max_retries as i64,
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                    task.completed_at.map(|dt| dt.to_rfc3339()),
                    serde_json::to_string(&task.subtask_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    task.agent_id.map(|id| id.to_string()),
                ),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("insert task failed: {e}")))?;

        debug!(task_id = %task.id, "task inserted");
        Ok(())
    }

    #[instrument(skip(self), fields(task_id = %id))]
    async fn get(&self, id: &Id) -> Result<Option<Task>> {
        let mut rows = self
            .conn
            .query(
                "SELECT * FROM tasks WHERE id = ?1 AND deleted_at IS NULL",
                params!(id.to_string()),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("get task failed: {e}")))?;

        match rows.next().await {
            Ok(Some(row)) => Ok(Some(task_from_row(&row)?)),
            Ok(None) => Ok(None),
            Err(e) => Err(AdpError::StoreError(format!("get task row failed: {e}"))),
        }
    }

    #[instrument(skip(self, task), fields(task_id = %task.id))]
    async fn update(&self, task: &Task) -> Result<()> {
        let sql = r#"
            UPDATE tasks SET
                parent_id = ?1,
                state = ?2,
                task_type = ?3,
                context = ?4,
                strategy = ?5,
                retry_count = ?6,
                max_retries = ?7,
                created_at = ?8,
                updated_at = ?9,
                completed_at = ?10,
                subtask_ids = ?11,
                agent_id = ?12
            WHERE id = ?13 AND deleted_at IS NULL
        "#;

        let affected = self
            .conn
            .execute(
                sql,
                params!(
                    task.parent_id.map(|id| id.to_string()),
                    task.state.to_string(),
                    &task.task_type,
                    serde_json::to_string(&task.context)
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    serde_json::to_string(&task.strategy)
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    task.retry_count as i64,
                    task.max_retries as i64,
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                    task.completed_at.map(|dt| dt.to_rfc3339()),
                    serde_json::to_string(&task.subtask_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>())
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    task.agent_id.map(|id| id.to_string()),
                    task.id.to_string(),
                ),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("update task failed: {e}")))?;

        if affected == 0 {
            return Err(AdpError::StoreError(format!(
                "task {} not found or deleted",
                task.id
            )));
        }

        debug!(task_id = %task.id, "task updated");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn list_all(&self) -> Result<Vec<Task>> {
        let mut rows = self
            .conn
            .query(
                "SELECT * FROM tasks WHERE deleted_at IS NULL ORDER BY created_at",
                params!(),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("list tasks failed: {e}")))?;

        let mut tasks = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            tasks.push(task_from_row(&row)?);
        }

        Ok(tasks)
    }
}

// ===================================================================
// EventStore implementation
// ===================================================================

#[async_trait]
impl EventStore for LibSqlEventStore {
    #[instrument(skip(self, event), fields(event_id = %event.id, task_id = %event.task_id))]
    async fn append(&self, event: &Event) -> Result<()> {
        let sql = r#"
            INSERT INTO events (
                id, task_id, event_type, from_state, to_state,
                payload, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#;

        self.conn
            .execute(
                sql,
                params!(
                    event.id.to_string(),
                    event.task_id.to_string(),
                    event_type_to_string(event.event_type),
                    event.from_state.map(|s| s.to_string()),
                    event.to_state.map(|s| s.to_string()),
                    serde_json::to_string(&event.payload)
                        .map_err(|e| AdpError::SerializationError(e.to_string()))?,
                    event.created_at.to_rfc3339(),
                ),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("insert event failed: {e}")))?;

        debug!(event_id = %event.id, "event appended");
        Ok(())
    }

    #[instrument(skip(self), fields(task_id = %task_id))]
    async fn get_by_task(&self, task_id: &Id) -> Result<Vec<Event>> {
        let mut rows = self
            .conn
            .query(
                "SELECT * FROM events WHERE task_id = ?1 ORDER BY created_at",
                params!(task_id.to_string()),
            )
            .await
            .map_err(|e| AdpError::StoreError(format!("get events failed: {e}")))?;

        let mut events = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            events.push(event_from_row(&row)?);
        }

        Ok(events)
    }
}

// ===================================================================
// Row mapping helpers
// ===================================================================

fn task_from_row(row: &libsql::Row) -> Result<Task> {
    let id_str: String = row.get(0).map_err(|e| map_row_err("id", e))?;
    let parent_id_str: Option<String> = row.get(1).map_err(|e| map_row_err("parent_id", e))?;
    let state_str: String = row.get(2).map_err(|e| map_row_err("state", e))?;
    let task_type: String = row.get(3).map_err(|e| map_row_err("task_type", e))?;
    let context_json: String = row.get(4).map_err(|e| map_row_err("context", e))?;
    let strategy_json: String = row.get(5).map_err(|e| map_row_err("strategy", e))?;
    let retry_count: i64 = row.get(6).map_err(|e| map_row_err("retry_count", e))?;
    let max_retries: i64 = row.get(7).map_err(|e| map_row_err("max_retries", e))?;
    let created_at_str: String = row.get(8).map_err(|e| map_row_err("created_at", e))?;
    let updated_at_str: String = row.get(9).map_err(|e| map_row_err("updated_at", e))?;
    let completed_at_str: Option<String> = row.get(10).map_err(|e| map_row_err("completed_at", e))?;
    let subtask_ids_json: String = row.get(11).map_err(|e| map_row_err("subtask_ids", e))?;
    let agent_id_str: Option<String> = row.get(12).map_err(|e| map_row_err("agent_id", e))?;

    let id = parse_ulid(&id_str)?;
    let parent_id = parent_id_str.map(|s| parse_ulid(&s)).transpose()?;
    let state = parse_task_state(&state_str)?;
    let context = serde_json::from_str(&context_json)
        .map_err(|e| AdpError::SerializationError(format!("context JSON: {e}")))?;
    let strategy = serde_json::from_str(&strategy_json)
        .map_err(|e| AdpError::SerializationError(format!("strategy JSON: {e}")))?;
    let created_at = parse_datetime(&created_at_str)?;
    let updated_at = parse_datetime(&updated_at_str)?;
    let completed_at = completed_at_str.map(|s| parse_datetime(&s)).transpose()?;
    let subtask_ids: Vec<String> = serde_json::from_str(&subtask_ids_json)
        .map_err(|e| AdpError::SerializationError(format!("subtask_ids JSON: {e}")))?;
    let subtask_ids = subtask_ids
        .into_iter()
        .map(|s| parse_ulid(&s))
        .collect::<Result<Vec<_>>>()?;
    let agent_id = agent_id_str.map(|s| parse_ulid(&s)).transpose()?;

    Ok(Task {
        id,
        parent_id,
        state,
        task_type,
        context,
        strategy,
        retry_count: retry_count as u32,
        max_retries: max_retries as u32,
        created_at,
        updated_at,
        completed_at,
        subtask_ids,
        agent_id,
    })
}

fn event_from_row(row: &libsql::Row) -> Result<Event> {
    let id_str: String = row.get(0).map_err(|e| map_row_err("id", e))?;
    let task_id_str: String = row.get(1).map_err(|e| map_row_err("task_id", e))?;
    let event_type_str: String = row.get(2).map_err(|e| map_row_err("event_type", e))?;
    let from_state_str: Option<String> = row.get(3).map_err(|e| map_row_err("from_state", e))?;
    let to_state_str: Option<String> = row.get(4).map_err(|e| map_row_err("to_state", e))?;
    let payload_json: String = row.get(5).map_err(|e| map_row_err("payload", e))?;
    let created_at_str: String = row.get(6).map_err(|e| map_row_err("created_at", e))?;

    let id = parse_ulid(&id_str)?;
    let task_id = parse_ulid(&task_id_str)?;
    let event_type = parse_event_type(&event_type_str)?;
    let from_state = from_state_str.map(|s| parse_task_state(&s)).transpose()?;
    let to_state = to_state_str.map(|s| parse_task_state(&s)).transpose()?;
    let payload = serde_json::from_str(&payload_json)
        .map_err(|e| AdpError::SerializationError(format!("payload JSON: {e}")))?;
    let created_at = parse_datetime(&created_at_str)?;

    Ok(Event {
        id,
        task_id,
        event_type,
        from_state,
        to_state,
        payload,
        created_at,
    })
}

// ===================================================================
// Parsing helpers
// ===================================================================

fn parse_ulid(s: &str) -> Result<Id> {
    Ulid::from_string(s)
        .map(Id::from)
        .map_err(|e| AdpError::SerializationError(format!("invalid ULID '{s}': {e}")))
}

fn parse_task_state(s: &str) -> Result<TaskState> {
    match s {
        "PENDING" => Ok(TaskState::Pending),
        "DELEGATED" => Ok(TaskState::Delegated),
        "EXECUTING" => Ok(TaskState::Executing),
        "AWAITING_SUBTASKS" => Ok(TaskState::AwaitingSubtasks),
        "AGGREGATING" => Ok(TaskState::Aggregating),
        "COMPLETED" => Ok(TaskState::Completed),
        "FAILED" => Ok(TaskState::Failed),
        "CANCELLED" => Ok(TaskState::Cancelled),
        _ => Err(AdpError::SerializationError(format!(
            "unknown task state: {s}"
        ))),
    }
}

fn parse_event_type(s: &str) -> Result<EventType> {
    match s {
        "STATE_TRANSITION" => Ok(EventType::StateTransition),
        "SUBTASK_CREATED" => Ok(EventType::SubtaskCreated),
        "DELEGATION_CREATED" => Ok(EventType::DelegationCreated),
        "DELEGATION_COMPLETED" => Ok(EventType::DelegationCompleted),
        "RETRY_TRIGGERED" => Ok(EventType::RetryTriggered),
        "CANCELLED" => Ok(EventType::Cancelled),
        _ => Err(AdpError::SerializationError(format!(
            "unknown event type: {s}"
        ))),
    }
}

fn event_type_to_string(et: EventType) -> String {
    match et {
        EventType::StateTransition => "STATE_TRANSITION".to_string(),
        EventType::SubtaskCreated => "SUBTASK_CREATED".to_string(),
        EventType::DelegationCreated => "DELEGATION_CREATED".to_string(),
        EventType::DelegationCompleted => "DELEGATION_COMPLETED".to_string(),
        EventType::RetryTriggered => "RETRY_TRIGGERED".to_string(),
        EventType::Cancelled => "CANCELLED".to_string(),
    }
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| AdpError::SerializationError(format!("invalid datetime '{s}': {e}")))
}

fn map_row_err(column: &str, e: libsql::Error) -> AdpError {
    AdpError::StoreError(format!("row get '{column}' failed: {e}"))
}
