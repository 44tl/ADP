//! Integration tests for the DAG scheduler.
//!
//! These tests exercise the full lifecycle: task creation, delegation,
//! execution, subtask spawning, parent blocking, completion, failure, and retry.

use adp_core::scheduler::{DagScheduler, SchedulerOutcome};
use adp_core::store::{InMemoryEventStore, InMemoryTaskStore};
use adp_core::task::{Id, Strategy, Task, TaskState};
use serde_json::json;

fn setup() -> (InMemoryTaskStore, InMemoryEventStore, DagScheduler<InMemoryTaskStore, InMemoryEventStore>) {
    let task_store = InMemoryTaskStore::default();
    let event_store = InMemoryEventStore::default();
    let scheduler = DagScheduler::new(task_store.clone(), event_store.clone());
    (task_store, event_store, scheduler)
}

#[tokio::test]
async fn full_leaf_task_lifecycle() {
    let (task_store, event_store, scheduler) = setup();

    let task = Task::builder("test.full_lifecycle").build();
    let task_id = task.id;
    scheduler.create_task(&task).await.unwrap();

    // Delegate
    let agent_id = Id::new();
    let outcome = scheduler.delegate_task(task_id, agent_id).await.unwrap();
    assert!(matches!(
        outcome,
        SchedulerOutcome::Transitioned {
            new_state: TaskState::Delegated,
            ..
        }
    ));

    // Start
    let outcome = scheduler.start_execution(task_id).await.unwrap();
    assert!(matches!(
        outcome,
        SchedulerOutcome::Transitioned {
            new_state: TaskState::Executing,
            ..
        }
    ));

    // Complete
    let outcome = scheduler
        .complete_task(task_id, json!({"status": "ok"}))
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        SchedulerOutcome::Transitioned {
            new_state: TaskState::Completed,
            ..
        }
    ));

    // Verify persistence
    let stored = task_store.get(&task_id).await.unwrap().unwrap();
    assert_eq!(stored.state, TaskState::Completed);
    assert!(stored.completed_at.is_some());

    let events = event_store.get_by_task(&task_id).await.unwrap();
    assert_eq!(events.len(), 3); // delegate, start, complete
}

#[tokio::test]
async fn parent_child_blocking() {
    let (task_store, _event_store, scheduler) = setup();

    // Create parent
    let parent = Task::builder("test.parent").build();
    let parent_id = parent.id;
    scheduler.create_task(&parent).await.unwrap();

    // Delegate and start parent
    scheduler.delegate_task(parent_id, Id::new()).await.unwrap();
    scheduler.start_execution(parent_id).await.unwrap();

    // Spawn subtasks
    let sub1 = Task::builder("test.sub1").parent_id(parent_id).build();
    let sub2 = Task::builder("test.sub2").parent_id(parent_id).build();
    let sub1_id = sub1.id;
    let sub2_id = sub2.id;

    let outcome = scheduler.spawn_subtasks(parent_id, vec![sub1, sub2]).await.unwrap();
    assert!(matches!(
        outcome,
        SchedulerOutcome::SubtasksSpawned { .. }
    ));

    // Parent should be AWAITING_SUBTASKS
    let parent = task_store.get(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::AwaitingSubtasks);

    // Complete first subtask
    scheduler.delegate_task(sub1_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub1_id).await.unwrap();
    scheduler.complete_task(sub1_id, json!({"result": 1})).await.unwrap();

    // Parent should still be waiting
    let parent = task_store.get(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::AwaitingSubtasks);

    // Complete second subtask — parent unblocks
    scheduler.delegate_task(sub2_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub2_id).await.unwrap();
    let outcome = scheduler
        .complete_task(sub2_id, json!({"result": 2}))
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        SchedulerOutcome::ParentUnblocked { parent_id: pid } if pid == parent_id
    ));

    let parent = task_store.get(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::Aggregating);

    // Aggregate and complete parent
    scheduler
        .aggregate_and_complete(parent_id, json!({"sum": 3}))
        .await
        .unwrap();
    let parent = task_store.get(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::Completed);
}

#[tokio::test]
async fn broadcast_cancel_losers() {
    let (task_store, _event_store, scheduler) = setup();

    let task = Task::builder("test.broadcast")
        .strategy(Strategy::Broadcast(adp_core::task::BroadcastStrategy {
            max_agents: 3,
        }))
        .build();
    let task_id = task.id;
    scheduler.create_task(&task).await.unwrap();

    // Simulate broadcast by spawning 3 subtasks
    let sub1 = Task::builder("test.broadcast_run").parent_id(task_id).build();
    let sub2 = Task::builder("test.broadcast_run").parent_id(task_id).build();
    let sub3 = Task::builder("test.broadcast_run").parent_id(task_id).build();
    let sub1_id = sub1.id;
    let sub2_id = sub2.id;
    let sub3_id = sub3.id;

    scheduler.delegate_task(task_id, Id::new()).await.unwrap();
    scheduler.start_execution(task_id).await.unwrap();
    scheduler.spawn_subtasks(task_id, vec![sub1, sub2, sub3]).await.unwrap();

    // Winner completes first
    scheduler.delegate_task(sub1_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub1_id).await.unwrap();
    scheduler.complete_task(sub1_id, json!({"winner": true})).await.unwrap();

    // Cancel losers
    scheduler
        .cancel_task(sub2_id, "broadcast loser".to_string())
        .await
        .unwrap();
    scheduler
        .cancel_task(sub3_id, "broadcast loser".to_string())
        .await
        .unwrap();

    // Verify losers are cancelled
    let sub2 = task_store.get(&sub2_id).await.unwrap().unwrap();
    assert_eq!(sub2.state, TaskState::Cancelled);
    let sub3 = task_store.get(&sub3_id).await.unwrap().unwrap();
    assert_eq!(sub3.state, TaskState::Cancelled);

    // Parent had one success and two cancellations.
    // Since not ALL cancelled, parent should remain AWAITING_SUBTASKS
    // (in a real BROADCAST implementation, the scheduler would detect
    // the winner and complete the parent immediately).
    let parent = task_store.get(&task_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::AwaitingSubtasks);
}

#[tokio::test]
async fn retry_exhaustion_for_leaf_task() {
    let (task_store, event_store, scheduler) = setup();

    let task = Task::builder("test.retry").max_retries(1).build();
    let task_id = task.id;
    scheduler.create_task(&task).await.unwrap();

    // First attempt
    scheduler.delegate_task(task_id, Id::new()).await.unwrap();
    scheduler.start_execution(task_id).await.unwrap();
    scheduler
        .fail_task(task_id, "timeout".to_string())
        .await
        .unwrap();

    let task = task_store.get(&task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Delegated); // auto-retried
    assert_eq!(task.retry_count, 1);

    // Second attempt fails
    scheduler.start_execution(task_id).await.unwrap();
    let outcome = scheduler
        .fail_task(task_id, "timeout again".to_string())
        .await
        .unwrap();

    let task = task_store.get(&task_id).await.unwrap().unwrap();
    assert_eq!(task.state, TaskState::Failed);
    assert_eq!(task.retry_count, 1);

    assert!(matches!(
        outcome,
        SchedulerOutcome::Transitioned {
            new_state: TaskState::Failed,
            ..
        }
    ));

    // Verify events
    let events = event_store.get_by_task(&task_id).await.unwrap();
    // delegate, start, fail, retry, start, fail = 6 transitions
    assert_eq!(events.len(), 6);
}

#[tokio::test]
async fn parent_task_does_not_auto_retry() {
    let (task_store, _event_store, scheduler) = setup();

    let parent = Task::builder("test.parent").max_retries(3).build();
    let parent_id = parent.id;
    scheduler.create_task(&parent).await.unwrap();

    scheduler.delegate_task(parent_id, Id::new()).await.unwrap();
    scheduler.start_execution(parent_id).await.unwrap();

    let sub = Task::builder("test.sub").parent_id(parent_id).build();
    let sub_id = sub.id;
    scheduler.spawn_subtasks(parent_id, vec![sub]).await.unwrap();

    // Fail the subtask — parent should fail but NOT auto-retry
    scheduler.delegate_task(sub_id, Id::new()).await.unwrap();
    scheduler.start_execution(sub_id).await.unwrap();
    scheduler
        .fail_task(sub_id, "subtask error".to_string())
        .await
        .unwrap();

    let parent = task_store.get(&parent_id).await.unwrap().unwrap();
    assert_eq!(parent.state, TaskState::Failed);
    assert_eq!(parent.retry_count, 0); // no auto-retry
}

#[tokio::test]
async fn get_dag_returns_all_descendants() {
    let (_task_store, _event_store, scheduler) = setup();

    let root = Task::builder("test.root").build();
    let root_id = root.id;
    scheduler.create_task(&root).await.unwrap();

    let a = Task::builder("test.a").parent_id(root_id).build();
    let b = Task::builder("test.b").parent_id(root_id).build();
    let a_id = a.id;
    let b_id = b.id;

    let a1 = Task::builder("test.a1").parent_id(a_id).build();
    let a1_id = a1.id;

    scheduler.delegate_task(root_id, Id::new()).await.unwrap();
    scheduler.start_execution(root_id).await.unwrap();
    scheduler.spawn_subtasks(root_id, vec![a, b]).await.unwrap();

    scheduler.delegate_task(a_id, Id::new()).await.unwrap();
    scheduler.start_execution(a_id).await.unwrap();
    scheduler.spawn_subtasks(a_id, vec![a1]).await.unwrap();

    let dag = scheduler.get_dag(root_id).await.unwrap();
    assert_eq!(dag.len(), 4);
    assert!(dag.contains_key(&root_id));
    assert!(dag.contains_key(&a_id));
    assert!(dag.contains_key(&b_id));
    assert!(dag.contains_key(&a1_id));
}

#[tokio::test]
async fn find_delegatable_tasks_respects_parent_state() {
    let (task_store, _event_store, scheduler) = setup();

    let parent = Task::builder("test.parent").build();
    let parent_id = parent.id;
    scheduler.create_task(&parent).await.unwrap();

    // Parent not yet spawned subtasks — child should not be delegatable
    let child = Task::builder("test.child").parent_id(parent_id).build();
    let child_id = child.id;
    scheduler.create_task(&child).await.unwrap();

    let ready = scheduler.find_delegatable_tasks().await.unwrap();
    assert!(!ready.iter().any(|t| t.id == child_id));
    assert!(ready.iter().any(|t| t.id == parent_id));

    // Now spawn the child
    scheduler.delegate_task(parent_id, Id::new()).await.unwrap();
    scheduler.start_execution(parent_id).await.unwrap();
    scheduler.spawn_subtasks(parent_id, vec![child]).await.unwrap();

    let ready = scheduler.find_delegatable_tasks().await.unwrap();
    assert!(ready.iter().any(|t| t.id == child_id));
}
