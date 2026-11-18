CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    root TEXT NOT NULL UNIQUE
);
CREATE TABLE workflows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT NOT NULL REFERENCES projects(id),
    request TEXT NOT NULL,
    baseline TEXT NOT NULL,
    fixture TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'interrupted', 'failed', 'awaiting_approval')),
    checkpoint TEXT NOT NULL CHECK (json_valid(checkpoint)),
    pruned_through INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE attempts (
    workflow_id INTEGER NOT NULL REFERENCES workflows(id),
    id INTEGER NOT NULL,
    task TEXT NOT NULL,
    agent TEXT NOT NULL,
    number INTEGER NOT NULL CHECK (number > 0),
    state TEXT NOT NULL CHECK (state IN ('active', 'completed', 'failed', 'cancelled', 'interrupted')),
    PRIMARY KEY (workflow_id, id),
    UNIQUE (workflow_id, task, number)
);
CREATE UNIQUE INDEX active_task ON attempts(workflow_id, task) WHERE state = 'active';
CREATE UNIQUE INDEX active_agent ON attempts(workflow_id, agent) WHERE state = 'active';
CREATE TABLE sessions (
    workflow_id INTEGER NOT NULL,
    attempt_id INTEGER NOT NULL,
    provider TEXT NOT NULL,
    native_id TEXT NOT NULL,
    PRIMARY KEY (workflow_id, attempt_id),
    UNIQUE (workflow_id, provider, native_id),
    FOREIGN KEY (workflow_id, attempt_id) REFERENCES attempts(workflow_id, id)
);
CREATE TABLE outbox (
    workflow_id INTEGER NOT NULL,
    attempt_id INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'acknowledged', 'finished', 'interrupted')),
    PRIMARY KEY (workflow_id, attempt_id),
    FOREIGN KEY (workflow_id, attempt_id) REFERENCES attempts(workflow_id, id)
);
CREATE TABLE inbox (
    workflow_id INTEGER NOT NULL,
    attempt_id INTEGER NOT NULL,
    outcome TEXT NOT NULL CHECK (json_valid(outcome)),
    PRIMARY KEY (workflow_id, attempt_id),
    FOREIGN KEY (workflow_id, attempt_id) REFERENCES attempts(workflow_id, id)
);
CREATE TABLE mailbox (
    workflow_id INTEGER NOT NULL,
    id INTEGER NOT NULL,
    from_attempt INTEGER NOT NULL,
    to_attempt INTEGER NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    delivered INTEGER NOT NULL DEFAULT 0 CHECK (delivered IN (0, 1)),
    PRIMARY KEY (workflow_id, id),
    FOREIGN KEY (workflow_id, from_attempt) REFERENCES attempts(workflow_id, id),
    FOREIGN KEY (workflow_id, to_attempt) REFERENCES attempts(workflow_id, id)
);
CREATE TABLE events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    workflow_id INTEGER NOT NULL REFERENCES workflows(id),
    payload TEXT NOT NULL CHECK (json_valid(payload))
);
CREATE INDEX workflow_events ON events(workflow_id, seq);
CREATE INDEX runnable_workflows ON workflows(status, id);
PRAGMA user_version = 1;
