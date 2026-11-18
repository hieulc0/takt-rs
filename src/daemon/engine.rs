use takt_rs::{
    domain::{
        event::{AttemptId, Event, TaskState, Verification},
        scheduler::Scheduler,
    },
    mock::MockBackend,
};

use super::{error, protocol::Status, store::Store, Result};

const MAX_TICKS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Claimed,
    Started,
    Acknowledged,
    BeforeCompletionCommit,
    CompletionCommitted,
}

pub struct Engine {
    pub store: Store,
    active: Option<Active>,
}

struct Active {
    id: i64,
    scheduler: Scheduler,
    backend: MockBackend,
    ticks: usize,
}

impl Engine {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            active: None,
        }
    }

    pub fn advance(&mut self, hook: &mut impl FnMut(Boundary, Option<AttemptId>)) -> Result<()> {
        if self.active.is_none() {
            let Some(id) = self.store.next_workflow()? else {
                return Ok(());
            };
            self.active = Some(Active {
                id,
                scheduler: self.store.load_scheduler(id)?,
                backend: MockBackend::with_session_counter(self.store.session_counter(id)?),
                ticks: 0,
            });
        }
        let work = self.active.as_mut().expect("loaded workflow");
        let assignments = work.scheduler.claim_ready();
        if !assignments.is_empty() {
            self.store
                .save(work.id, Status::Running, &mut work.scheduler, vec![])?;
            hook(Boundary::Claimed, None);
        }
        for assignment in assignments {
            let attempt = assignment.attempt;
            let script = crate::demo::script(&assignment.task).map_err(error)?;
            let session = work
                .backend
                .start(assignment, script)
                .map_err(|err| error(format!("{err:?}")))?;
            hook(Boundary::Started, Some(attempt));
            self.store
                .acknowledge(work.id, attempt, session.native_id())?;
            hook(Boundary::Acknowledged, Some(attempt));
        }
        let reported = work
            .backend
            .tick(&mut work.scheduler)
            .map_err(|err| error(format!("{err:?}")))?;
        for &attempt in &reported {
            work.scheduler.verify(attempt, Verification::Passed);
        }
        work.ticks += 1;
        let (status, extra) = if work.scheduler.all_completed() {
            (
                Status::AwaitingApproval,
                vec![Event::SimulatedApprovalPending],
            )
        } else if work
            .scheduler
            .task_states()
            .any(|(_, state)| matches!(state, TaskState::Interrupted { .. }))
        {
            work.scheduler.interrupt_active();
            (Status::Interrupted, vec![])
        } else if !work.scheduler.has_active_attempts()
            && !work
                .scheduler
                .task_states()
                .any(|(_, state)| state == TaskState::Ready)
        {
            (Status::Failed, vec![])
        } else if work.ticks >= MAX_TICKS {
            work.scheduler.interrupt_active();
            (Status::Interrupted, vec![])
        } else {
            (Status::Running, vec![])
        };
        if !reported.is_empty() {
            hook(Boundary::BeforeCompletionCommit, None);
        }
        if !work.scheduler.events().is_empty() || status != Status::Running {
            self.store
                .save(work.id, status, &mut work.scheduler, extra)?;
        }
        if !reported.is_empty() {
            hook(Boundary::CompletionCommitted, None);
        }
        if status != Status::Running {
            self.active = None;
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        if let Some(work) = &mut self.active {
            self.store.interrupt(work.id, &mut work.scheduler)?;
        }
        Ok(())
    }
}
