-- Initial schema for ADP Core.
-- Uses libSQL (SQLite-compatible) with soft deletes only.

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    state TEXT NOT NULL,
    task_type TEXT NOT NULL,
    context TEXT NOT NULL,        -- JSON
    strategy TEXT NOT NULL,       -- JSON
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    completed_at DATETIME,
    subtask_ids TEXT NOT NULL,    -- JSON array of ULIDs
    agent_id TEXT,
    deleted_at DATETIME,
    FOREIGN KEY (parent_id) REFERENCES tasks(id)
);

CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    capabilities TEXT NOT NULL,   -- JSON array
    metadata TEXT NOT NULL,       -- JSON
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME
);

CREATE TABLE delegations (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    FOREIGN KEY (task_id) REFERENCES tasks(id),
    FOREIGN KEY (agent_id) REFERENCES agents(id)
);

CREATE TABLE events (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    from_state TEXT,
    to_state TEXT,
    payload TEXT NOT NULL,        -- JSON
    created_at DATETIME NOT NULL,
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

CREATE TABLE memories (
    id TEXT PRIMARY KEY,
    agent_id TEXT,
    task_id TEXT,
    content TEXT NOT NULL,
    embedding_id TEXT,            -- reference to Qdrant vector ID
    metadata TEXT NOT NULL,       -- JSON
    created_at DATETIME NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id),
    FOREIGN KEY (task_id) REFERENCES tasks(id)
);

-- Indexes
CREATE INDEX idx_tasks_state ON tasks(state);
CREATE INDEX idx_tasks_parent ON tasks(parent_id);
CREATE INDEX idx_tasks_agent ON tasks(agent_id);
CREATE INDEX idx_tasks_deleted ON tasks(deleted_at);
CREATE INDEX idx_events_task ON events(task_id);
CREATE INDEX idx_events_created ON events(created_at);
CREATE INDEX idx_delegations_task ON delegations(task_id);
CREATE INDEX idx_delegations_agent ON delegations(agent_id);
CREATE INDEX idx_memories_agent ON memories(agent_id);
CREATE INDEX idx_memories_task ON memories(task_id);
