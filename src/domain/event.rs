#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AttemptId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment {
    pub attempt: AttemptId,
    pub task: String,
    pub agent: String,
    pub number: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Queued,
    Ready,
    Running,
    Blocked,
    Verifying,
    Completed,
    Failed,
    Cancelling,
    Cancelled,
    Interrupted { cancellation_requested: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    ResultReported,
    /// A temporary failure known to have occurred before any side effects.
    TransientFailure,
    PermanentFailure,
    Cancelled,
    Crashed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    Passed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    HelpRequest,
    HelpAnswer { request: MessageId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub from: AttemptId,
    pub to: AttemptId,
    pub kind: MessageKind,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Ready(String),
    Claimed(Assignment),
    StateChanged {
        task: String,
        state: TaskState,
    },
    ProviderOutcome {
        attempt: AttemptId,
        outcome: Outcome,
    },
    Verified {
        attempt: AttemptId,
        result: Verification,
    },
    MessageQueued(Message),
    MessageDelivered {
        message: MessageId,
        to: AttemptId,
    },
    RetryScheduled(String),
    SimulatedApprovalPending,
}
