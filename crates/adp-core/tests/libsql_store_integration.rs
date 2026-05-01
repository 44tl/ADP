//! Integration tests for libSQL-backed stores.
//!
//! These tests exercise the full persistence layer: migrations, task CRUD,
//! event append, and scheduler integration with a real libSQL database.

use adp_core::db::open_database;
use adp_core::scheduler::{DagScheduler, SchedulerOutcome};
use adp_core::store::{EventStore, TaskStore};
use adp_core::store_libsql::{LibSqlEventStore, LibSqlTaskStore};
use adp_core::task::{EventType, Id, Task, TaskState};
use serde_json::json;
use std::path::PathBuf;

fn temp_db_path() -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("adp_test_{}.db", ulid::Ulid::new()));
    path
}

async fn setup_stores() -> (LibSqlTaskStore, LibSqlEventStore, PathBuf) {
    let path = temp_db_path();
    let conn = open_database(&path).await.unwrap();
    let task_store = LibSqlTaskStore::new(conn.clone());
    let event_store = LibSqlEventStore::new(conn);
    (task_store, event_store, path)
}

#[tokio::test]
async fn libsql_task_crud() {
    let (store, _event_store, _path) = setup_stores().await;

    let task = Task::builder("test.libsql.crud")
        .context(json!({"key": "value"}))
        .max_retries(5)
        .build();
    let id = task.id;

    // Insert
    store.insert(&task).await.unwrap();

    // Get
    let fetched = store.get(&id).await.unwrap().unwrap();
    assert_eq!(fetched.id, id);
    assert_eq!(fetched.task_type, "test.libsql.crud");
    assert_eq!(fetched.state, TaskState::Pending);
    assert_eq!(fetched.context, json!({"key": "value"}));
    assert_eq!(fetched.max_retries, 5);
    assert!(fetched.subtask_ids.is_empty());

    // Update
    let mut updated = fetched.clone();
    updated.state = TaskState::Delegated;
    updated.agent_id = Some(Id::new());
    store.update(&updated).await.unwrap();

    let fetched2 = store.get(&id).await.unwrap().unwrap();
    assert_eq!(fetched2.state, TaskState::Delegated);
    assert!(fetched2.agent_id.is_some());

    // List all
    let all = store.list_all().await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].id, id);

    // Insert duplicate fails
    let result = store.insert(&task).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn libsql_event_append_and_query() {
    let (task_store, event_store, _path) = setup_stores().await;

    let task = Task::builder("test.libsql.events").build();
    let task_id = task.id;
    task_store.insert(&task).await.unwrap();

    let event = adp_core::task::Event {
        id: Id::new(),
        task_id,
        event_type: EventType::StateTransition,
        from_state: Some(TaskState::Pending),
        to_state: Some(TaskState::Delegated),
        payload: json!({"agent_id": Id::new().to_string()}),
        created_at: chrono::Utc::now(),
    };

    event_store.append(&event).await.unwrap();

    let events = event_store.get_by_task(&task_id).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, EventType::StateTransition);
    assert_eq!(events[0].from_state, Some(TaskState::Pending));
    assert_eq!(events[0].to_state, Some(TaskState::Delegated));
}

#[tokio::test]
async fn libsql_scheduler_full_dag() {
    let (task_store, event_store, _path) = setup_stores().await;
    let scheduler = DagScheduler::new(task_store.clone(), event_store.clone());

    // Root task
    let root = Task::builder("test.root").build();
    let root_id = root.id;
    scheduler.create_task(&root).await.unwrap();

    // Delegate and start root
    scheduler.delegate_task(root_id, Id::new()).await.unwrap();
    scheduler.start_execution(root_id).await.unwrap();

    // Spawn subtasks
    let sub1 = Task::builder("test.sub1").parent_id(root_id).build();
    let sub2 = Task::builder("test.sub2").parent_id(root_id).build();
    let sub1_id = sub1.id;
    let sub2_id = sub2.id;

    scheduler.spawn_subtasks(root_id, vec![sub1, sub2]).await.unwrap();

    // Complete subtasks
    scheduler.delegate_task(sub1_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub1_id).await.unwrap();
    let outcome = scheduler
        .complete_task(sub1_id, json!({"result": 1}))
        .await
        .unwrap();
    assert!(matches!(outcome, SchedulerOutcome::Transitioned { .. }));

    scheduler.delegate_task(sub2_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub2_id).await.unwrap();
    let outcome = scheduler
        .complete_task(sub2_id, json!({"result": 2}))
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        SchedulerOutcome::ParentUnblocked { parent_id } if parent_id == root_id
    ));

    // Root should be aggregating
    let root = task_store.get(&root_id).await.unwrap().unwrap();
    assert_eq!(root.state, TaskState::Aggregating);

    // Aggregate and complete
    scheduler
        .aggregate_and_complete(root_id, json!({"sum": 3}))
        .await
        .unwrap();

    let root = task_store.get(&root_id).await.unwrap().unwrap();
    assert_eq!(root.state, TaskState::Completed);
    assert!(root.completed_at.is_some());

    // Verify events were persisted
    let events = event_store.get_by_task(&root_id).await.unwrap();
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| e.event_type == EventType::SubtaskCreated));
}

#[tokio::test]
async fn libsql_task_with_subtask_ids_roundtrips() {
    let (store, _event_store, _path) = setup_stores().await;

    let mut task = Task::builder("test.subtasks").build();
    task.subtask_ids = vec![Id::new(), Id::new(), Id::new()];
    let id = task.id;

    store.insert(&task).await.unwrap();
    let fetched = store.get(&id).await.unwrap().unwrap();
    assert_eq!(fetched.subtask_ids.len(), 3);
    assert_eq!(fetched.subtask_ids, task.subtask_ids);
}

#[tokio::test]
async fn libsql_get_nonexistent_returns_none() {
    let (store, _event_store, _path) = setup_stores().await;
    let result = store.get(&Id::new()).await.unwrap();
    assert!(result.is_none());
}
