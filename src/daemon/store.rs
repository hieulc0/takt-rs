use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use takt_rs::domain::{
    event::{Assignment, AttemptId, Event, TaskState},
    scheduler::{Checkpoint, Scheduler},
};

use super::protocol::{Entry, Response, Status, Workflow};
use super::{error, Result};

const FIXTURE: &str = "two-developers-v1";
const RETAIN_EVENTS: i64 = 256;
pub(super) const EVENT_PAGE: usize = 32;

pub struct Store(Connection);

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let mut db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;",
        )?;
        let version: u32 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version == 0 {
            let tx = db.transaction()?;
            tx.execute_batch(include_str!("../../migrations/0001_initial.sql"))?;
            tx.commit()?;
        } else if version != 1 {
            return Err(error(format!("unsupported database version {version}")));
        }
        let check: String = db.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" || db.prepare("PRAGMA foreign_key_check")?.exists([])? {
            return Err(error("SQLite integrity check failed"));
        }
        Ok(Self(db))
    }

    pub fn add_project(&self, id: &str, root: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO projects(id, root) VALUES (?1, ?2)",
            params![id, root],
        )?;
        Ok(())
    }

    pub fn project_root(&self, id: &str) -> Result<String> {
        self.0
            .query_row("SELECT root FROM projects WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| error("unknown project"))
    }

    pub fn submit(
        &mut self,
        project: &str,
        request: &str,
        baseline: &str,
        scheduler: &mut Scheduler,
    ) -> Result<i64> {
        let tx = self.0.transaction()?;
        tx.execute(
            "INSERT INTO workflows(project_id, request, baseline, fixture, status, checkpoint)
             VALUES (?1, ?2, ?3, ?4, 'queued', ?5)",
            params![
                project,
                request,
                baseline,
                FIXTURE,
                serde_json::to_string(scheduler.checkpoint())?
            ],
        )?;
        let id = tx.last_insert_rowid();
        append(&tx, id, scheduler.drain_events())?;
        tx.commit()?;
        Ok(id)
    }

    pub fn workflow(&self, id: i64) -> Result<Workflow> {
        let (project, request, baseline, state): (String, String, String, String) = self
            .0
            .query_row(
                "SELECT project_id, request, baseline, status FROM workflows WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| error("unknown workflow"))?;
        let status = match state.as_str() {
            "queued" => Status::Queued,
            "running" => Status::Running,
            "interrupted" => Status::Interrupted,
            "failed" => Status::Failed,
            "awaiting_approval" => Status::AwaitingApproval,
            _ => return Err(error("invalid stored workflow status")),
        };
        let scheduler = self.load_scheduler(id)?;
        let tasks = scheduler
            .task_states()
            .map(|(task, state)| (task.to_owned(), state))
            .collect();
        Ok(Workflow {
            id,
            project,
            request,
            baseline,
            status,
            tasks,
        })
    }

    pub fn events(&self, id: i64, after: i64) -> Result<Response> {
        let pruned_through: i64 = self
            .0
            .query_row(
                "SELECT pruned_through FROM workflows WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| error("unknown workflow"))?;
        let mut statement = self.0.prepare(
            "SELECT seq, payload FROM events WHERE workflow_id = ?1 AND seq > ?2 ORDER BY seq LIMIT ?3",
        )?;
        let mut rows = statement.query(params![id, after, EVENT_PAGE])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            let payload: String = row.get(1)?;
            entries.push(Entry {
                seq: row.get(0)?,
                event: serde_json::from_str(&payload)?,
            });
        }
        Ok(Response::Events {
            entries,
            pruned_through,
        })
    }

    pub fn next_workflow(&self) -> Result<Option<i64>> {
        Ok(self.0.query_row(
            "SELECT id FROM workflows WHERE status IN ('queued', 'running') ORDER BY id LIMIT 1",
            [], |row| row.get(0),
        ).optional()?)
    }

    pub fn load_scheduler(&self, id: i64) -> Result<Scheduler> {
        let (fixture, serialized): (String, String) = self.0.query_row(
            "SELECT fixture, checkpoint FROM workflows WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if fixture != FIXTURE {
            return Err(error("unsupported stored mock fixture"));
        }
        let checkpoint: Checkpoint = serde_json::from_str(&serialized)?;
        let mut scheduler = crate::demo::scheduler().map_err(error)?;
        scheduler.restore_checkpoint(checkpoint);
        Ok(scheduler)
    }

    pub fn session_counter(&self, id: i64) -> Result<u64> {
        Ok(self.0.query_row(
            "SELECT coalesce(max(CAST(native_id AS INTEGER)), 0) FROM sessions WHERE workflow_id = ?1 AND provider = 'mock'",
            [id], |row| row.get(0),
        )?)
    }

    /// Acknowledge separately from claiming so recovery can distinguish the two writes.
    pub fn acknowledge(&mut self, id: i64, attempt: AttemptId, native_id: u64) -> Result<()> {
        let tx = self.0.transaction()?;
        let changed = tx.execute(
            "UPDATE outbox SET state = 'acknowledged' WHERE workflow_id = ?1 AND attempt_id = ?2 AND state = 'pending'",
            params![id, attempt.0],
        )?;
        if changed != 1 {
            return Err(error("dispatch has no pending outbox entry"));
        }
        tx.execute(
            "INSERT INTO sessions(workflow_id, attempt_id, provider, native_id) VALUES (?1, ?2, 'mock', ?3)",
            params![id, attempt.0, native_id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn recover(&mut self) -> Result<()> {
        // Only the selected workflow can own mock sessions. A running workflow with
        // no active attempts can continue from its safe checkpoint.
        let ids = {
            let mut statement = self
                .0
                .prepare("SELECT id FROM workflows WHERE status = 'running'")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<i64>>>()?
        };
        for id in ids {
            let mut scheduler = self.load_scheduler(id)?;
            self.interrupt(id, &mut scheduler)?;
        }
        Ok(())
    }

    pub fn interrupt(&mut self, id: i64, scheduler: &mut Scheduler) -> Result<()> {
        if scheduler.has_active_attempts() {
            scheduler.interrupt_active();
            self.save(id, Status::Interrupted, scheduler, vec![])?;
        }
        Ok(())
    }

    pub fn save(
        &mut self,
        id: i64,
        status: Status,
        scheduler: &mut Scheduler,
        extra: Vec<Event>,
    ) -> Result<()> {
        let tx = self.0.transaction()?;
        tx.execute(
            "UPDATE workflows SET status = ?2, checkpoint = ?3 WHERE id = ?1",
            params![
                id,
                status.as_str(),
                serde_json::to_string(scheduler.checkpoint())?
            ],
        )?;
        let mut events = scheduler.drain_events();
        events.extend(extra);
        for event in &events {
            project_event(&tx, id, event)?;
        }
        close_attempts(&tx, id, scheduler)?;
        append(&tx, id, events)?;
        tx.commit()?;
        Ok(())
    }
}

fn project_event(tx: &Transaction<'_>, id: i64, event: &Event) -> Result<()> {
    match event {
        Event::Claimed(Assignment {
            attempt,
            task,
            agent,
            number,
        }) => {
            tx.execute(
                "INSERT INTO attempts(workflow_id, id, task, agent, number, state) VALUES (?1, ?2, ?3, ?4, ?5, 'active')",
                params![id, attempt.0, task, agent, number],
            )?;
            tx.execute(
                "INSERT INTO outbox(workflow_id, attempt_id, state) VALUES (?1, ?2, 'pending')",
                params![id, attempt.0],
            )?;
        }
        Event::ProviderOutcome { attempt, outcome } => {
            tx.execute(
                "INSERT INTO inbox(workflow_id, attempt_id, outcome) VALUES (?1, ?2, ?3)",
                params![id, attempt.0, serde_json::to_string(outcome)?],
            )?;
        }
        Event::MessageQueued(message) => {
            tx.execute(
                "INSERT INTO mailbox(workflow_id, id, from_attempt, to_attempt, payload) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, message.id.0, message.from.0, message.to.0, serde_json::to_string(message)?],
            )?;
        }
        Event::MessageDelivered { message, .. } => {
            tx.execute(
                "UPDATE mailbox SET delivered = 1 WHERE workflow_id = ?1 AND id = ?2",
                params![id, message.0],
            )?;
        }
        _ => {}
    }
    Ok(())
}

fn close_attempts(tx: &Transaction<'_>, id: i64, scheduler: &Scheduler) -> Result<()> {
    let mut statement =
        tx.prepare("SELECT id, task FROM attempts WHERE workflow_id = ?1 AND state = 'active'")?;
    let rows = statement.query_map([id], |row| {
        Ok((AttemptId(row.get(0)?), row.get::<_, String>(1)?))
    })?;
    let active = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for (attempt, task) in active {
        if scheduler.attempt_state(attempt).is_some() {
            continue;
        }
        let (state, outbox) = match scheduler
            .state(&task)
            .map_err(|err| error(format!("{err:?}")))?
        {
            TaskState::Completed => ("completed", "finished"),
            TaskState::Cancelled => ("cancelled", "finished"),
            TaskState::Interrupted { .. } => ("interrupted", "interrupted"),
            TaskState::Failed | TaskState::Ready => ("failed", "finished"),
            _ => return Err(error("active attempt has no terminal state")),
        };
        tx.execute(
            "UPDATE attempts SET state = ?3 WHERE workflow_id = ?1 AND id = ?2",
            params![id, attempt.0, state],
        )?;
        tx.execute(
            "UPDATE outbox SET state = ?3 WHERE workflow_id = ?1 AND attempt_id = ?2",
            params![id, attempt.0, outbox],
        )?;
    }
    Ok(())
}

fn append(tx: &Transaction<'_>, id: i64, events: Vec<Event>) -> Result<()> {
    for event in events {
        tx.execute(
            "INSERT INTO events(workflow_id, payload) VALUES (?1, ?2)",
            params![id, serde_json::to_string(&event)?],
        )?;
    }
    let cutoff: Option<i64> = tx
        .query_row(
            "SELECT seq FROM events WHERE workflow_id = ?1 ORDER BY seq DESC LIMIT 1 OFFSET ?2",
            params![id, RETAIN_EVENTS],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(cutoff) = cutoff {
        tx.execute(
            "DELETE FROM events WHERE workflow_id = ?1 AND seq <= ?2",
            params![id, cutoff],
        )?;
        tx.execute(
            "UPDATE workflows SET pruned_through = ?2 WHERE id = ?1",
            params![id, cutoff],
        )?;
    }
    Ok(())
}
