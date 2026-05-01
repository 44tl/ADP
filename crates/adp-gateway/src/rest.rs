//! REST API (axum).
//!
//! Provides HTTP endpoints for task CRUD, delegation, and queries.

use crate::error::{GatewayError, Result};
use adp_core::scheduler::DagScheduler;
use adp_core::store::{EventStore, TaskStore};
use adp_core::task::{Id, Task};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tracing::{info, instrument};

/// REST API state.
#[derive(Debug, Clone)]
pub struct ApiState<S: TaskStore, E: EventStore> {
    scheduler: Arc<DagScheduler<S, E>>,
}

/// Create task request.
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub task_type: String,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub parent_id: Option<String>,
}

/// Task response.
#[derive(Debug, Serialize)]
pub struct TaskResponse {
    pub id: String,
    pub state: String,
    pub task_type: String,
    pub created_at: String,
}

impl From<Task> for TaskResponse {
    fn from(task: Task) -> Self {
        Self {
            id: task.id.to_string(),
            state: task.state.to_string(),
            task_type: task.task_type,
            created_at: task.created_at.to_rfc3339(),
        }
    }
}

/// Build the REST router.
pub fn router<S: TaskStore, E: EventStore>(scheduler: DagScheduler<S, E>) -> Router {
    let state = ApiState {
        scheduler: Arc::new(scheduler),
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/tasks", post(create_task::<S, E>))
        .route("/tasks/:id", get(get_task::<S, E>))
        .with_state(state)
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

#[instrument(skip(state, req))]
async fn create_task<S: TaskStore, E: EventStore>(
    State(state): State<ApiState<S, E>>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let mut builder = Task::builder(&req.task_type).context(req.context);

    if let Some(parent_id) = req.parent_id {
        let id = Id::from(
            ulid::Ulid::from_string(&parent_id)
                .map_err(|_| StatusCode::BAD_REQUEST)?,
        );
        builder = builder.parent_id(id);
    }

    let task = builder.build();
    let response = TaskResponse::from(task.clone());

    state
        .scheduler
        .create_task(&task)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    info!(task_id = %task.id, "task created via REST");
    Ok(Json(response))
}

#[instrument(skip(state))]
async fn get_task<S: TaskStore, E: EventStore>(
    State(state): State<ApiState<S, E>>,
    Path(id): Path<String>,
) -> Result<Json<TaskResponse>, StatusCode> {
    let id = Id::from(
        ulid::Ulid::from_string(&id).map_err(|_| StatusCode::BAD_REQUEST)?,
    );

    let dag = state
        .scheduler
        .get_dag(id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let task = dag.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(TaskResponse::from(task.clone())))
}

/// REST server.
#[derive(Debug)]
pub struct RestServer {
    addr: std::net::SocketAddr,
}

impl RestServer {
    pub fn new(addr: std::net::SocketAddr) -> Self {
        Self { addr }
    }

    /// Start the REST server.
    pub async fn serve<S: TaskStore, E: EventStore>(self, scheduler: DagScheduler<S, E>) -> Result<()> {
        let app = router(scheduler);
        let listener = tokio::net::TcpListener::bind(self.addr)
            .await
            .map_err(|e| GatewayError::Rest(format!("bind failed: {e}")))?;

        info!(addr = %self.addr, "REST server starting");
        axum::serve(listener, app)
            .await
            .map_err(|e| GatewayError::Rest(format!("serve failed: {e}")))?;

        Ok(())
    }
}
