use super::*;
use crate::domain::{
    dag::TaskSpec,
    event::MessageKind,
    plan::{Limits, Plan, PlannedTask, Role, PLAN_VERSION},
};

fn task(id: &str, role: Role, deps: &[&str]) -> PlannedTask {
    PlannedTask {
        graph: TaskSpec {
            id: id.into(),
            depends_on: deps.iter().map(|s| (*s).into()).collect(),
        },
        role,
        priority: 0,
        allowed_paths: vec![],
        acceptance: vec!["Fixture verified".into()],
    }
}

fn agent(id: &str, role: Role) -> Agent {
    Agent {
        id: id.into(),
        role,
    }
}

fn plan(tasks: Vec<PlannedTask>, parallel: usize) -> ValidatedPlan {
    Plan {
        version: PLAN_VERSION,
        tasks,
    }
    .validate(
        Limits {
            max_tasks: 5,
            max_depth: 5,
            max_parallel: parallel,
            max_attempts: 2,
        },
        &[],
    )
    .unwrap()
}

fn workflow(max_parallel: usize) -> Scheduler {
    Scheduler::new(
        plan(
            vec![
                task("a", Role::Developer, &[]),
                task("b", Role::Developer, &[]),
                task("c", Role::Verifier, &["a", "b"]),
            ],
            max_parallel,
        ),
        vec![
            agent("ada", Role::Developer),
            agent("lin", Role::Developer),
            agent("checker", Role::Verifier),
        ],
    )
    .unwrap()
}

fn complete(scheduler: &mut Scheduler, attempt: AttemptId) {
    assert!(scheduler.report_outcome(attempt, Outcome::ResultReported));
    assert!(scheduler.verify(attempt, Verification::Passed));
}

#[test]
fn parallel_roots_are_claimed_once_and_join_waits_for_verified_results() {
    let mut scheduler = workflow(2);
    assert_eq!(
        scheduler.events(),
        &[Event::Ready("a".into()), Event::Ready("b".into())]
    );
    let claims = scheduler.claim_ready();
    assert_eq!(claims.len(), 2);
    let (a, b) = (&claims[0], &claims[1]);
    assert_eq!(
        (&*a.task, &*a.agent, &*b.task, &*b.agent),
        ("a", "ada", "b", "lin")
    );
    assert!(scheduler.claim_ready().is_empty());
    complete(&mut scheduler, a.attempt);
    assert!(scheduler.report_outcome(b.attempt, Outcome::ResultReported));
    assert_eq!(scheduler.state("b").unwrap(), TaskState::Verifying);
    assert_eq!(scheduler.state("c").unwrap(), TaskState::Queued);
    assert!(scheduler.claim_ready().is_empty());

    let count = scheduler.events().len();
    assert!(!scheduler.report_outcome(b.attempt, Outcome::ResultReported));
    assert_eq!(scheduler.events().len(), count);
    assert!(scheduler.verify(b.attempt, Verification::Passed));
    let count = scheduler.events().len();
    assert!(!scheduler.verify(b.attempt, Verification::Passed));
    assert!(!scheduler.report_outcome(b.attempt, Outcome::ResultReported));
    assert_eq!(scheduler.events().len(), count);
    let verifier = scheduler.claim_ready();
    assert_eq!((&*verifier[0].task, &*verifier[0].agent), ("c", "checker"));
}

#[test]
fn global_and_agent_slots_prevent_second_claim() {
    let mut scheduler = workflow(1);
    let first = scheduler.claim_ready();
    assert_eq!(first.len(), 1);
    assert_eq!(scheduler.state("b").unwrap(), TaskState::Ready);
    assert!(scheduler.claim_ready().is_empty());
    complete(&mut scheduler, first[0].attempt);
    assert_eq!(scheduler.claim_ready()[0].task, "b");

    let mut scheduler = Scheduler::new(
        plan(
            vec![
                task("a", Role::Developer, &[]),
                task("b", Role::Developer, &[]),
            ],
            2,
        ),
        vec![
            agent("ada", Role::Developer),
            agent("checker", Role::Verifier),
        ],
    )
    .unwrap();
    assert_eq!(scheduler.claim_ready().len(), 1);
    assert!(scheduler.claim_ready().is_empty());
}

#[test]
fn role_coverage_and_unique_agents_are_required() {
    let proposal = || plan(vec![task("a", Role::Developer, &[])], 1);
    assert!(
        matches!(Scheduler::new(proposal(), vec![agent("checker", Role::Verifier)]),
        Err(SchedulerError::MissingRole(id)) if id == "a")
    );
    assert!(
        matches!(Scheduler::new(proposal(), vec![agent("ada", Role::Developer), agent("ada", Role::Verifier)]),
        Err(SchedulerError::InvalidAgent(id)) if id == "ada")
    );
}

#[test]
fn priorities_win_then_older_ready_tasks_take_precedence() {
    let mut high = task("high", Role::Developer, &[]);
    high.priority = 1;
    let mut scheduler = Scheduler::new(
        plan(
            vec![
                task("new", Role::Developer, &["high"]),
                task("old", Role::Developer, &[]),
                high,
            ],
            1,
        ),
        vec![agent("ada", Role::Developer)],
    )
    .unwrap();
    let first = scheduler.claim_ready();
    assert_eq!(first[0].task, "high");
    complete(&mut scheduler, first[0].attempt);
    assert_eq!(scheduler.claim_ready()[0].task, "old");
}

#[test]
fn failed_verification_never_unlocks_a_dependent() {
    let mut scheduler = workflow(2);
    let roots = scheduler.claim_ready();
    complete(&mut scheduler, roots[0].attempt);
    assert!(scheduler.report_outcome(roots[1].attempt, Outcome::ResultReported));
    assert!(scheduler.verify(roots[1].attempt, Verification::Failed));
    assert_eq!(scheduler.state("b").unwrap(), TaskState::Failed);
    assert_eq!(scheduler.state("c").unwrap(), TaskState::Queued);
    assert!(scheduler.claim_ready().is_empty());
}

#[test]
fn transient_retry_has_a_new_attempt_and_stops_at_budget() {
    let mut scheduler = workflow(2);
    let claims = scheduler.claim_ready();
    let first = claims[0].attempt;
    assert!(scheduler.report_outcome(first, Outcome::TransientFailure));
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Ready);
    let retry = scheduler.claim_ready();
    assert_eq!(retry.len(), 1);
    let retry = &retry[0];
    assert_eq!((&*retry.task, retry.number), ("a", 2));
    assert_ne!(retry.attempt, first);
    assert!(!scheduler.report_outcome(first, Outcome::ResultReported));
    assert!(scheduler.report_outcome(retry.attempt, Outcome::TransientFailure));
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Failed);
    assert!(scheduler.report_outcome(claims[1].attempt, Outcome::PermanentFailure));
    assert_eq!(scheduler.state("b").unwrap(), TaskState::Failed);
    assert!(scheduler.claim_ready().is_empty());
}

#[test]
fn runtime_loss_requires_reconciliation_and_preserves_cancel_intent() {
    let mut scheduler = workflow(2);
    let roots = scheduler.claim_ready();
    assert!(scheduler.report_outcome(roots[0].attempt, Outcome::ResultReported));
    assert!(scheduler.cancel("b").unwrap());
    scheduler.interrupt_active();
    assert_eq!(
        scheduler.state("a").unwrap(),
        TaskState::Interrupted {
            cancellation_requested: false
        }
    );
    assert_eq!(
        scheduler.state("b").unwrap(),
        TaskState::Interrupted {
            cancellation_requested: true
        }
    );
    assert!(scheduler.claim_ready().is_empty());
    assert!(!scheduler.verify(roots[0].attempt, Verification::Passed));
    assert!(scheduler.reconcile_for_retry("b").is_err());
    scheduler.reconcile_for_retry("a").unwrap();
    let retry = scheduler.claim_ready();
    assert_eq!((&*retry[0].task, retry[0].number), ("a", 2));
    assert!(scheduler.report_outcome(retry[0].attempt, Outcome::Crashed));
    assert!(scheduler.reconcile_for_retry("a").is_err());
    assert_eq!(scheduler.state("c").unwrap(), TaskState::Queued);
}

#[test]
fn cancellation_requires_acknowledgement_for_active_work() {
    let mut scheduler = workflow(1);
    let a = scheduler.claim_ready()[0].attempt;
    assert!(scheduler.cancel("a").unwrap());
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Cancelling);
    assert!(!scheduler.cancel("a").unwrap());
    assert!(!scheduler.report_outcome(a, Outcome::ResultReported));
    assert!(!scheduler.report_outcome(a, Outcome::TransientFailure));
    assert!(scheduler.claim_ready().is_empty());
    assert!(scheduler.report_outcome(a, Outcome::Cancelled));
    assert!(!scheduler.report_outcome(a, Outcome::Cancelled));
    assert!(scheduler.cancel("b").unwrap()); // Ready.
    assert!(scheduler.cancel("c").unwrap()); // Queued.
    for task in ["a", "b", "c"] {
        assert_eq!(scheduler.state(task).unwrap(), TaskState::Cancelled);
    }
    assert!(scheduler.claim_ready().is_empty());
}

#[test]
fn verification_can_be_cancelled_but_verified_completion_is_immutable() {
    let mut scheduler = workflow(2);
    let roots = scheduler.claim_ready();
    let (a, b) = (roots[0].attempt, roots[1].attempt);
    assert!(scheduler.report_outcome(a, Outcome::ResultReported));
    complete(&mut scheduler, b);
    assert!(scheduler.cancel("a").unwrap());
    assert!(!scheduler.verify(a, Verification::Passed));
    assert!(!scheduler.cancel("b").unwrap());
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Cancelled);
    assert_eq!(scheduler.state("b").unwrap(), TaskState::Completed);
}

#[test]
fn help_is_addressed_and_only_delivered_answers_unblock() {
    let mut scheduler = workflow(2);
    let roots = scheduler.claim_ready();
    let (a, b) = (roots[0].attempt, roots[1].attempt);
    let request = scheduler.request_help(a, b, "field?".into()).unwrap();
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Blocked);
    assert!(scheduler.request_help(b, a, "cycle".into()).is_err());
    assert!(!scheduler.report_outcome(a, Outcome::ResultReported));
    assert!(scheduler.answer_help(b, request, "early".into()).is_err());
    assert_eq!(scheduler.take_inbox(b)[0].id, request);
    assert!(scheduler.take_inbox(b).is_empty());
    assert!(scheduler
        .answer_help(a, request, "wrong sender".into())
        .is_err());
    let answer = scheduler.answer_help(b, request, "name".into()).unwrap();
    assert!(scheduler
        .answer_help(b, request, "duplicate".into())
        .is_err());
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Blocked);
    complete(&mut scheduler, b);
    let delivery = scheduler.take_inbox(a);
    assert_eq!(delivery[0].id, answer);
    assert_eq!(delivery[0].kind, MessageKind::HelpAnswer { request });
    assert_eq!(scheduler.state("a").unwrap(), TaskState::Running);
    assert!(scheduler.claim_ready().is_empty());
    complete(&mut scheduler, a);
    assert_eq!(scheduler.state("c").unwrap(), TaskState::Ready);
}
