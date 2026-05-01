# ADP Protocol Specification

## Overview

Agent Delegation Protocol (ADP) is a local-first orchestration layer for autonomous
AI agent teams. This document describes the wire format and lifecycle semantics.

## Task Lifecycle

Tasks are the atomic unit of work. Every task has a unique ULID identifier
and moves through the following states:

```
PENDING → DELEGATED → EXECUTING → AWAITING_SUBTASKS → AGGREGATING → COMPLETED
   |          |           |              |                |            |
   └──────────┴───────────┴──────────────┴────────────────┴────────────┘
   ↓
FAILED (→ may retry → DELEGATED)
```

### States

| State | Description |
|-------|-------------|
| `PENDING` | Task created, awaiting delegation. |
| `DELEGATED` | Assigned to an agent. |
| `EXECUTING` | Agent is actively working on the task. |
| `AWAITING_SUBTASKS` | Task spawned subtasks; blocked until completion. |
| `AGGREGATING` | All subtasks complete; results being combined. |
| `COMPLETED` | Terminal. Task finished successfully. |
| `FAILED` | Terminal (unless retried). Task failed. |
| `CANCELLED` | Terminal. Task explicitly cancelled (e.g., BROADCAST losers). |

### State Machine Rules

1. A task may spawn 0-N subtasks. The parent blocks in `AWAITING_SUBTASKS`
   until all children reach a terminal state.
2. `FAILED` tasks may retry up to `max_retries`, transitioning back to `DELEGATED`.
3. On retry, the task's `agent_id` is cleared and `completed_at` is reset.
4. Every state transition produces an `Event` in the audit log.
5. Context (JSON) is opaque and append-only. Agents may add keys; they never
   mutate upstream context.

## Delegation Strategies

### SINGLE
Assign to exactly one agent. Simplest strategy.

### BROADCAST
Assign to N agents (`max_agents`). First completion wins; other agents'
tasks are cancelled.

### VOTE
Assign to N agents. Require quorum agreement (default 67%). Ties trigger
re-delegation.

### PIPELINE
Chain of stages. Output of stage N is appended to context input for stage N+1.

### MAP_REDUCE
Spawn M map tasks. A reduce task aggregates all map outputs.

## Wire Format

Protobuf schemas live in `crates/adp-core/proto/adp/core/v1/`.

- Package: `adp.core.v1`
- Every message has `google.protobuf.Timestamp created_at`.
- Use `bytes` for opaque payloads; never `string` for binary data.
- Use `oneof` for strategy-specific configuration.

## Event Log

The `events` table is an immutable append-only log. Every state transition,
delegation, retry, and cancellation is recorded.

Key fields:
- `id`: ULID of the event itself.
- `task_id`: ULID of the affected task.
- `event_type`: Classification (`STATE_TRANSITION`, `SUBTASK_CREATED`, etc.).
- `from_state` / `to_state`: For transition events.
- `payload`: JSON blob with command-specific context.
- `created_at`: Timestamp.
