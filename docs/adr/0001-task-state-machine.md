# ADR 0001: Task State Machine

## Status
Accepted

## Context
Every task in ADP moves through a well-defined lifecycle. We need deterministic,
auditable state transitions with clear retry semantics. The state machine must be:

1. **Pure** — transition logic is separate from orchestration.
2. **Replayable** — every decision is recorded as an event.
3. **Testable** — property-based tests can exhaustively verify invariants.

## Decision
We implement a pure [`TaskStateMachine`](../../crates/adp-core/src/state_machine.rs) that:

- Validates all transitions via an explicit allow-list.
- Produces an [`Event`](../../crates/adp-core/src/task.rs) for every state change.
- Separates transition logic from the [`DagScheduler`](../../crates/adp-core/src/scheduler.rs),
  which handles parent-child blocking and strategy rules.

### States

```
PENDING → DELEGATED → EXECUTING → AWAITING_SUBTASKS → AGGREGATING → COMPLETED
   |          |           |              |                |            |
   └──────────┴───────────┴──────────────┴────────────────┴────────────┘
   ↓
FAILED (→ may retry → DELEGATED)
```

Plus `CANCELLED` as an explicit terminal state for BROADCAST losers.

### Key Rules

1. A task may spawn 0-N subtasks. The parent blocks in `AWAITING_SUBTASKS` until all
   children reach a terminal state (`COMPLETED`, `FAILED`, or `CANCELLED`).
2. `FAILED` tasks may retry up to `max_retries`, transitioning back to `DELEGATED`.
   On retry, `agent_id` is cleared and `completed_at` is reset.
3. Leaf tasks auto-retry on failure. Parent tasks do **not** auto-retry; they remain
   `FAILED` until explicitly retried by higher-level orchestration.
4. Every transition produces an `Event` with `from_state`, `to_state`, and a JSON payload.

## Consequences

- **Positive**: Deterministic, testable, replayable. Events provide a complete audit trail.
- **Positive**: Invalid transitions are rejected at compile-time (via `TransitionCommand`)
  and at runtime (via `validate_transition`).
- **Negative**: Slightly more boilerplate than free-form state updates.
- **Negative**: Parent unblocking requires O(N) subtask checks. This is acceptable for
  local-first use cases; sharded stores would need a different approach.
