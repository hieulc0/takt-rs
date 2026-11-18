//! Offline provider with one scripted turn per session per tick.

use std::collections::{BTreeMap, VecDeque};

use crate::domain::{
    event::{Assignment, AttemptId, MessageId, MessageKind, Outcome, TaskState},
    scheduler::{Scheduler, SchedulerError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SessionId(u64);

impl SessionId {
    pub fn native_id(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub enum Step {
    Yield,
    Ask {
        task: String,
        body: String,
    },
    /// Answer the oldest request that still has a waiting requester.
    Answer {
        body: String,
    },
    Finish(Outcome),
}

#[derive(Debug, PartialEq, Eq)]
pub enum MockError {
    DuplicateSession(AttemptId),
    ScriptExhausted(AttemptId),
    MissingPeer(String),
    Routing(SchedulerError),
}

struct Session {
    assignment: Assignment,
    script: VecDeque<Step>,
    requests: VecDeque<MessageId>,
}

#[derive(Default)]
pub struct MockBackend {
    next_session: u64,
    sessions: BTreeMap<SessionId, Session>,
}

impl MockBackend {
    pub fn with_session_counter(next_session: u64) -> Self {
        Self {
            next_session,
            ..Self::default()
        }
    }

    pub fn start(
        &mut self,
        assignment: Assignment,
        script: Vec<Step>,
    ) -> Result<SessionId, MockError> {
        if self
            .sessions
            .values()
            .any(|session| session.assignment.attempt == assignment.attempt)
        {
            return Err(MockError::DuplicateSession(assignment.attempt));
        }
        self.next_session += 1;
        let id = SessionId(self.next_session);
        self.sessions.insert(
            id,
            Session {
                assignment,
                script: script.into(),
                requests: VecDeque::new(),
            },
        );
        Ok(id)
    }

    /// Returns attempts whose provider result needs independent verification.
    pub fn tick(&mut self, scheduler: &mut Scheduler) -> Result<Vec<AttemptId>, MockError> {
        let peers: BTreeMap<_, _> = self
            .sessions
            .values()
            .map(|session| (session.assignment.task.clone(), session.assignment.attempt))
            .collect();
        let mut reported = Vec::new();
        for session in self.sessions.values_mut() {
            let attempt = session.assignment.attempt;
            if scheduler.attempt_state(attempt) == Some(TaskState::Cancelling) {
                scheduler.report_outcome(attempt, Outcome::Cancelled);
                continue;
            }
            // Mail only crosses the provider boundary between scripted turns.
            for message in scheduler.take_inbox(attempt) {
                if message.kind == MessageKind::HelpRequest {
                    session.requests.push_back(message.id);
                }
            }
            if scheduler.attempt_state(attempt) != Some(TaskState::Running) {
                continue;
            }
            let step = session
                .script
                .pop_front()
                .ok_or(MockError::ScriptExhausted(attempt))?;
            match step {
                Step::Yield => {}
                Step::Ask { task, body } => {
                    let peer = peers
                        .get(&task)
                        .copied()
                        .ok_or(MockError::MissingPeer(task))?;
                    scheduler
                        .request_help(attempt, peer, body)
                        .map_err(MockError::Routing)?;
                }
                Step::Answer { body } => {
                    while let Some(request) = session.requests.pop_front() {
                        if scheduler
                            .answer_help(attempt, request, body.clone())
                            .is_ok()
                        {
                            break;
                        }
                    }
                }
                Step::Finish(outcome) => {
                    if scheduler.report_outcome(attempt, outcome)
                        && outcome == Outcome::ResultReported
                    {
                        reported.push(attempt);
                    }
                }
            }
        }
        self.sessions.retain(|_, session| {
            matches!(
                scheduler.attempt_state(session.assignment.attempt),
                Some(TaskState::Running | TaskState::Blocked | TaskState::Cancelling)
            )
        });
        Ok(reported)
    }
}

#[cfg(test)]
mod tests;
