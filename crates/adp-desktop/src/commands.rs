//! Tauri command handlers.
//!
//! These functions are invoked from the frontend JavaScript/TypeScript.
//! They bridge the React UI to the Rust ADP core.

use crate::state::AppState;
use adp_core::task::{Id, Task, TaskState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;
use tracing::{info, instrument};

/// Response wrapper for commands.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(msg.into()),
        }
    }
}

/// Create a new task.
#[tauri::command]
#[instrument(skip(state))]
pub async fn create_task(
    state: State<'_, AppState>,
    task_type: String,
    context: Value,
) -> Result<ApiResponse<String>, String> {
    let task = Task::builder(&task_type).context(context).build();
    let id = task.id.to_string();

    state
        .scheduler
        .create_task(&task)
        .await
        .map_err(|e| e.to_string())?;

    info!(task_id = %id, "task created via desktop UI");
    Ok(ApiResponse::ok(id))
}

/// Get a task by ID.
#[tauri::command]
#[instrument(skip(state))]
pub async fn get_task(
    state: State<'_, AppState>,
    id: String,
) -> Result<ApiResponse<TaskView>, String> {
    let id = Id::from(
        ulid::Ulid::from_string(&id).map_err(|e| e.to_string())?,
    );

    let dag = state
        .scheduler
        .get_dag(id)
        .await
        .map_err(|e| e.to_string())?;

    let task = dag.get(&id).ok_or("task not found")?;

    Ok(ApiResponse::ok(TaskView::from(task.clone())))
}

/// List all tasks.
#[tauri::command]
#[instrument(skip(state))]
pub async fn list_tasks(
    state: State<'_, AppState>,
) -> Result<ApiResponse<Vec<TaskView>>, String> {
    let tasks = state
        .scheduler
        .find_delegatable_tasks()
        .await
        .map_err(|e| e.to_string())?;

    let views: Vec<TaskView> = tasks.into_iter().map(TaskView::from).collect();
    Ok(ApiResponse::ok(views))
}

/// Delegate a task to an agent.
#[tauri::command]
#[instrument(skip(state))]
pub async fn delegate_task(
    state: State<'_, AppState>,
    task_id: String,
    agent_id: String,
) -> Result<ApiResponse<String>, String> {
    let task_id = Id::from(
        ulid::Ulid::from_string(&task_id).map_err(|e| e.to_string())?,
    );
    let agent_id = Id::from(
        ulid::Ulid::from_string(&agent_id).map_err(|e| e.to_string())?,
    );

    state
        .scheduler
        .delegate_task(task_id, agent_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ApiResponse::ok("delegated".to_string()))
}

/// Cancel a task.
#[tauri::command]
#[instrument(skip(state))]
pub async fn cancel_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<ApiResponse<String>, String> {
    let task_id = Id::from(
        ulid::Ulid::from_string(&task_id).map_err(|e| e.to_string())?,
    );

    state
        .scheduler
        .cancel_task(task_id, "user cancelled".to_string())
        .await
        .map_err(|e| e.to_string())?;

    Ok(ApiResponse::ok("cancelled".to_string()))
}

/// View of a task for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub id: String,
    pub state: String,
    pub task_type: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub retry_count: u32,
}

impl From<Task> for TaskView {
    fn from(task: Task) -> Self {
        Self {
            id: task.id.to_string(),
            state: task.state.to_string(),
            task_type: task.task_type,
            parent_id: task.parent_id.map(|id| id.to_string()),
            created_at: task.created_at.to_rfc3339(),
            updated_at: task.updated_at.to_rfc3339(),
            retry_count: task.retry_count,
        }
    }
}
