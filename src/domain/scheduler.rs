//! Single-writer scheduling and transitions with no runtime or I/O dependencies.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use super::event::{
    Assignment, AttemptId, Event, Message, MessageId, Outcome, TaskState, Verification,
};
use super::plan::{Agent, ValidatedPlan};

mod messaging;

#[derive(Debug, PartialEq, Eq)]
pub enum SchedulerError {
    InvalidAgent(String),
    MissingRole(String),
    UnknownTask(String),
    InactiveAttempt(AttemptId),
    InvalidHelpRoute,
    InvalidHelpAnswer(MessageId),
    CannotReconcile(String),
}

#[derive(Serialize, Deserialize)]
struct Task {
    state: TaskState,
    attempts: u32,
    ready_since: usize,
    requested_help: bool,
}

#[derive(Serialize, Deserialize)]
struct HelpRequest {
    message: Message,
    delivered: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Checkpoint {
    tasks: Vec<Task>,
    active: BTreeMap<AttemptId, (usize, usize)>,
    next_attempt: u64,
    next_message: u64,
    inbox: BTreeMap<AttemptId, Vec<Message>>,
    requests: BTreeMap<MessageId, HelpRequest>,
    next_ready: usize,
}

impl Checkpoint {
    fn mark_ready_order(&mut self, task: usize) {
        self.tasks[task].ready_since = self.next_ready;
        self.next_ready += 1;
    }
}

pub struct Scheduler {
    plan: ValidatedPlan,
    agents: Vec<Agent>,
    checkpoint: Checkpoint,
    events: Vec<Event>,
}

impl Scheduler {
    pub fn new(plan: ValidatedPlan, agents: Vec<Agent>) -> Result<Self, SchedulerError> {
        let mut ids = HashSet::new();
        for agent in &agents {
            if agent.id.trim().is_empty() || !ids.insert(&agent.id) {
                return Err(SchedulerError::InvalidAgent(agent.id.clone()));
            }
        }
        for task in &plan.tasks {
            if !agents.iter().any(|agent| agent.role == task.role) {
                return Err(SchedulerError::MissingRole(task.graph.id.clone()));
            }
        }
        let tasks = plan
            .tasks
            .iter()
            .map(|_| Task {
                state: TaskState::Queued,
                attempts: 0,
                ready_since: 0,
                requested_help: false,
            })
            .collect();
        let mut scheduler = Self {
            plan,
            agents,
            checkpoint: Checkpoint {
                tasks,
                active: BTreeMap::new(),
                next_attempt: 1,
                next_message: 1,
                inbox: BTreeMap::new(),
                requests: BTreeMap::new(),
                next_ready: 0,
            },
            events: Vec::new(),
        };
        scheduler.refresh_ready();
        Ok(scheduler)
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// Restores state for the same validated plan and agent roster.
    pub fn restore_checkpoint(&mut self, checkpoint: Checkpoint) {
        self.checkpoint = checkpoint;
        self.events.clear();
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn task_states(&self) -> impl Iterator<Item = (&str, TaskState)> {
        self.plan
            .tasks
            .iter()
            .zip(&self.checkpoint.tasks)
            .map(|(spec, task)| (spec.graph.id.as_str(), task.state))
    }

    pub fn has_active_attempts(&self) -> bool {
        !self.checkpoint.active.is_empty()
    }

    pub fn state(&self, task: &str) -> Result<TaskState, SchedulerError> {
        Ok(self.checkpoint.tasks[self.task_index(task)?].state)
    }

    pub fn attempt_state(&self, attempt: AttemptId) -> Option<TaskState> {
        self.checkpoint
            .active
            .get(&attempt)
            .map(|&(task, _)| self.checkpoint.tasks[task].state)
    }

    pub fn all_completed(&self) -> bool {
        self.checkpoint
            .tasks
            .iter()
            .all(|task| task.state == TaskState::Completed)
    }

    /// Older ready tasks win ties at the same priority. Each agent has one slot.
    pub fn claim_ready(&mut self) -> Vec<Assignment> {
        let mut ready: Vec<_> = (0..self.checkpoint.tasks.len())
            .filter(|&index| self.checkpoint.tasks[index].state == TaskState::Ready)
            .collect();
        ready.sort_by_key(|&index| {
            (
                std::cmp::Reverse(self.plan.tasks[index].priority),
                self.checkpoint.tasks[index].ready_since,
            )
        });
        let mut occupied: HashSet<_> = self
            .checkpoint
            .active
            .values()
            .map(|&(_, agent)| agent)
            .collect();
        let mut assignments = Vec::new();
        for task in ready {
            if self.checkpoint.active.len() == self.plan.limits.max_parallel {
                break;
            }
            let spec = &self.plan.tasks[task];
            let Some(agent) =
                self.agents.iter().enumerate().position(|(index, agent)| {
                    agent.role == spec.role && !occupied.contains(&index)
                })
            else {
                continue;
            };
            let attempt = AttemptId(self.checkpoint.next_attempt);
            self.checkpoint.next_attempt += 1;
            self.checkpoint.tasks[task].attempts += 1;
            self.checkpoint.tasks[task].requested_help = false;
            let assignment = Assignment {
                attempt,
                task: spec.graph.id.clone(),
                agent: self.agents[agent].id.clone(),
                number: self.checkpoint.tasks[task].attempts,
            };
            self.checkpoint.active.insert(attempt, (task, agent));
            occupied.insert(agent);
            self.events.push(Event::Claimed(assignment.clone()));
            self.transition(task, TaskState::Running);
            assignments.push(assignment);
        }
        assignments
    }

    /// Applies a provider terminal outcome once, ignoring stale or duplicate signals.
    pub fn report_outcome(&mut self, attempt: AttemptId, outcome: Outcome) -> bool {
        let Some(&(task, _)) = self.checkpoint.active.get(&attempt) else {
            return false;
        };
        let state = self.checkpoint.tasks[task].state;
        let cancellation_requested = state == TaskState::Cancelling;
        let next = match (state, outcome) {
            (TaskState::Running, Outcome::ResultReported) => TaskState::Verifying,
            (
                TaskState::Running | TaskState::Blocked,
                Outcome::TransientFailure | Outcome::PermanentFailure,
            ) => TaskState::Failed,
            (
                TaskState::Running | TaskState::Blocked | TaskState::Cancelling,
                Outcome::Cancelled,
            ) => TaskState::Cancelled,
            (TaskState::Running | TaskState::Blocked | TaskState::Cancelling, Outcome::Crashed) => {
                TaskState::Interrupted {
                    cancellation_requested,
                }
            }
            _ => return false,
        };
        self.events
            .push(Event::ProviderOutcome { attempt, outcome });
        self.transition(task, next);
        if next != TaskState::Verifying {
            self.release(attempt);
        }
        if outcome == Outcome::TransientFailure && self.can_retry(task) {
            self.retry(task);
        }
        true
    }

    /// Verification is supplied by the trusted caller, separately from provider output.
    pub fn verify(&mut self, attempt: AttemptId, result: Verification) -> bool {
        if self.attempt_state(attempt) != Some(TaskState::Verifying) {
            return false;
        }
        let (task, _) = self.checkpoint.active[&attempt];
        self.events.push(Event::Verified { attempt, result });
        let next = match result {
            Verification::Passed => TaskState::Completed,
            Verification::Failed => TaskState::Failed,
        };
        self.transition(task, next);
        self.release(attempt);
        self.refresh_ready();
        true
    }

    pub fn cancel(&mut self, task: &str) -> Result<bool, SchedulerError> {
        let task = self.task_index(task)?;
        match self.checkpoint.tasks[task].state {
            TaskState::Queued | TaskState::Ready => self.transition(task, TaskState::Cancelled),
            TaskState::Verifying => {
                let attempt = self
                    .checkpoint
                    .active
                    .iter()
                    .find(|(_, (index, _))| *index == task)
                    .map(|(&attempt, _)| attempt)
                    .expect("verifying task has an attempt");
                self.release(attempt);
                self.transition(task, TaskState::Cancelled);
            }
            TaskState::Running | TaskState::Blocked => self.transition(task, TaskState::Cancelling),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Models recovery from a runtime loss while the in-memory scheduler survives.
    pub fn interrupt_active(&mut self) {
        let attempts: Vec<_> = self.checkpoint.active.keys().copied().collect();
        for attempt in attempts {
            let (task, _) = self.checkpoint.active[&attempt];
            let cancellation_requested = self.checkpoint.tasks[task].state == TaskState::Cancelling;
            self.transition(
                task,
                TaskState::Interrupted {
                    cancellation_requested,
                },
            );
            self.release(attempt);
        }
    }

    /// Call only when reconciliation establishes that repeating the task is safe.
    pub fn reconcile_for_retry(&mut self, task: &str) -> Result<(), SchedulerError> {
        let index = self.task_index(task)?;
        if self.checkpoint.tasks[index].state
            != (TaskState::Interrupted {
                cancellation_requested: false,
            })
            || !self.can_retry(index)
        {
            return Err(SchedulerError::CannotReconcile(task.into()));
        }
        self.retry(index);
        Ok(())
    }

    fn task_index(&self, id: &str) -> Result<usize, SchedulerError> {
        self.plan
            .tasks
            .iter()
            .position(|task| task.graph.id == id)
            .ok_or_else(|| SchedulerError::UnknownTask(id.into()))
    }

    fn task_id(&self, task: usize) -> String {
        self.plan.tasks[task].graph.id.clone()
    }

    fn transition(&mut self, task: usize, state: TaskState) {
        self.checkpoint.tasks[task].state = state;
        self.events.push(Event::StateChanged {
            task: self.task_id(task),
            state,
        });
    }

    fn refresh_ready(&mut self) {
        let completed = self
            .plan
            .tasks
            .iter()
            .zip(&self.checkpoint.tasks)
            .filter(|(_, task)| task.state == TaskState::Completed)
            .map(|(spec, _)| spec.graph.id.as_str())
            .collect();
        let ready: HashSet<_> = self.plan.graph.ready(&completed).into_iter().collect();
        for index in 0..self.checkpoint.tasks.len() {
            if self.checkpoint.tasks[index].state == TaskState::Queued
                && ready.contains(self.plan.tasks[index].graph.id.as_str())
            {
                self.checkpoint.tasks[index].state = TaskState::Ready;
                self.checkpoint.mark_ready_order(index);
                self.events.push(Event::Ready(self.task_id(index)));
            }
        }
    }

    fn can_retry(&self, task: usize) -> bool {
        self.checkpoint.tasks[task].attempts < self.plan.limits.max_attempts
    }

    fn retry(&mut self, task: usize) {
        self.events.push(Event::RetryScheduled(self.task_id(task)));
        self.checkpoint.mark_ready_order(task);
        self.transition(task, TaskState::Ready);
    }

    fn release(&mut self, attempt: AttemptId) {
        self.checkpoint.active.remove(&attempt);
        self.checkpoint.inbox.remove(&attempt);
        self.checkpoint
            .requests
            .retain(|_, help| help.message.from != attempt && help.message.to != attempt);
    }
}

#[cfg(test)]
mod tests;
