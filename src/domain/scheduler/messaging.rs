use super::{HelpRequest, Scheduler, SchedulerError};
use crate::domain::event::{AttemptId, Event, Message, MessageId, MessageKind, TaskState};

impl Scheduler {
    /// A simulated attempt can ask one currently running peer for help.
    pub fn request_help(
        &mut self,
        from: AttemptId,
        to: AttemptId,
        body: String,
    ) -> Result<MessageId, SchedulerError> {
        let &(task, _) = self
            .checkpoint
            .active
            .get(&from)
            .ok_or(SchedulerError::InactiveAttempt(from))?;
        if from == to
            || self.checkpoint.tasks[task].state != TaskState::Running
            || self.checkpoint.tasks[task].requested_help
            || self.attempt_state(to) != Some(TaskState::Running)
        {
            return Err(SchedulerError::InvalidHelpRoute);
        }
        let message = self.enqueue(from, to, MessageKind::HelpRequest, body);
        let id = message.id;
        self.checkpoint.requests.insert(
            id,
            HelpRequest {
                message,
                delivered: false,
            },
        );
        self.checkpoint.tasks[task].requested_help = true;
        self.transition(task, TaskState::Blocked);
        Ok(id)
    }

    pub fn answer_help(
        &mut self,
        from: AttemptId,
        request: MessageId,
        body: String,
    ) -> Result<MessageId, SchedulerError> {
        let Some(help) = self.checkpoint.requests.get(&request) else {
            return Err(SchedulerError::InvalidHelpAnswer(request));
        };
        let to = help.message.from;
        if !help.delivered
            || help.message.to != from
            || self.attempt_state(from) != Some(TaskState::Running)
            || self.attempt_state(to) != Some(TaskState::Blocked)
        {
            return Err(SchedulerError::InvalidHelpAnswer(request));
        }
        self.checkpoint.requests.remove(&request);
        Ok(self
            .enqueue(from, to, MessageKind::HelpAnswer { request }, body)
            .id)
    }

    /// The runtime drains mail before a turn. A delivered answer resumes its requester.
    pub fn take_inbox(&mut self, attempt: AttemptId) -> Vec<Message> {
        let state = self.attempt_state(attempt);
        if !matches!(state, Some(TaskState::Running | TaskState::Blocked)) {
            return Vec::new();
        }
        let mut delivered = Vec::new();
        let mut pending = Vec::new();
        for message in self.checkpoint.inbox.remove(&attempt).unwrap_or_default() {
            if message.kind == MessageKind::HelpRequest {
                let Some(help) = self.checkpoint.requests.get_mut(&message.id) else {
                    continue;
                };
                if state == Some(TaskState::Blocked) {
                    pending.push(message);
                    continue;
                }
                help.delivered = true;
            }
            self.events.push(Event::MessageDelivered {
                message: message.id,
                to: attempt,
            });
            if matches!(message.kind, MessageKind::HelpAnswer { .. }) {
                let (task, _) = self.checkpoint.active[&attempt];
                self.transition(task, TaskState::Running);
            }
            delivered.push(message);
        }
        if !pending.is_empty() {
            self.checkpoint.inbox.insert(attempt, pending);
        }
        delivered
    }

    fn enqueue(
        &mut self,
        from: AttemptId,
        to: AttemptId,
        kind: MessageKind,
        body: String,
    ) -> Message {
        let message = Message {
            id: MessageId(self.checkpoint.next_message),
            from,
            to,
            kind,
            body,
        };
        self.checkpoint.next_message += 1;
        self.checkpoint
            .inbox
            .entry(to)
            .or_default()
            .push(message.clone());
        self.events.push(Event::MessageQueued(message.clone()));
        message
    }
}
