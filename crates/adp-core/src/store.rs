//! Persistence traits and in-memory implementations.
//!
//! Production implementations will use libSQL (Turso embedded) via `refinery`
//! migrations. The traits here are intentionally minimal and async to allow
//! pluggable backends.

use crate::error::Result;
use crate::task::{Event, Id, Task};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Persistence interface for [`Task`] records.
#[async_trait]
pub trait TaskStore: Send + Sync + 'static {
    /// Insert a new task. Fails if the id already exists.
    async fn insert(&self, task: &Task) -> Result<()>;

    /// Retrieve a task by id.
    async fn get(&self, id: &Id) -> Result<Option<Task>>;

    /// Update an existing task. Fails if the id does not exist.
    async fn update(&self, task: &Task) -> Result<()>;

    /// List all tasks in the store.
    async fn list_all(&self) -> Result<Vec<Task>>;
}

/// Persistence interface for [`Event`] audit records.
#[async_trait]
pub trait EventStore: Send + Sync + 'static {
    /// Append an event to the log.
    async fn append(&self, event: &Event) -> Result<()>;

    /// Retrieve all events for a given task, ordered by creation time.
    async fn get_by_task(&self, task_id: &Id) -> Result<Vec<Event>>;
}

// ===================================================================
// In-memory implementations for unit / integration testing
// ===================================================================

/// In-memory [`TaskStore`] backed by a `RwLock<HashMap>`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryTaskStore {
    tasks: Arc<RwLock<HashMap<Id, Task>>>,
}

#[async_trait]
impl TaskStore for InMemoryTaskStore {
    async fn insert(&self, task: &Task) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if tasks.contains_key(&task.id) {
            return Err(crate::error::AdpError::StoreError(format!(
                "task {} already exists",
                task.id
            )));
        }
        tasks.insert(task.id, task.clone());
        Ok(())
    }

    async fn get(&self, id: &Id) -> Result<Option<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.get(id).cloned())
    }

    async fn update(&self, task: &Task) -> Result<()> {
        let mut tasks = self.tasks.write().await;
        if !tasks.contains_key(&task.id) {
            return Err(crate::error::AdpError::StoreError(format!(
                "task {} does not exist",
                task.id
            )));
        }
        tasks.insert(task.id, task.clone());
        Ok(())
    }

    async fn list_all(&self) -> Result<Vec<Task>> {
        let tasks = self.tasks.read().await;
        Ok(tasks.values().cloned().collect())
    }
}

/// In-memory [`EventStore`] backed by a `RwLock<Vec>`.
#[derive(Debug, Clone, Default)]
pub struct InMemoryEventStore {
    events: Arc<RwLock<Vec<Event>>>,
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn append(&self, event: &Event) -> Result<()> {
        let mut events = self.events.write().await;
        events.push(event.clone());
        Ok(())
    }

    async fn get_by_task(&self, task_id: &Id) -> Result<Vec<Event>> {
        let events = self.events.read().await;
        Ok(events
            .iter()
            .filter(|e| e.task_id == *task_id)
            .cloned()
            .collect())
    }
}
