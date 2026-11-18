use super::*;
use crate::domain::{
    dag::TaskSpec,
    event::Verification,
    plan::{Agent, Limits, Plan, PlannedTask, Role, PLAN_VERSION},
};

fn scheduler() -> Scheduler {
    let plan = Plan {
        version: PLAN_VERSION,
        tasks: vec![PlannedTask {
            graph: TaskSpec {
                id: "work".into(),
                depends_on: vec![],
            },
            role: Role::Developer,
            priority: 0,
            allowed_paths: vec![],
            acceptance: vec!["Mock check".into()],
        }],
    }
    .validate(
        Limits {
            max_tasks: 1,
            max_depth: 1,
            max_parallel: 1,
            max_attempts: 2,
        },
        &[],
    )
    .unwrap();
    Scheduler::new(
        plan,
        vec![Agent {
            id: "worker".into(),
            role: Role::Developer,
        }],
    )
    .unwrap()
}

#[test]
fn scripted_failure_then_success_uses_a_fresh_session_and_needs_verification() {
    let mut scheduler = scheduler();
    let mut mock = MockBackend::default();
    let first = scheduler.claim_ready().pop().unwrap();
    let first_session = mock
        .start(first.clone(), vec![Step::Finish(Outcome::TransientFailure)])
        .unwrap();
    assert_eq!(
        mock.start(first.clone(), vec![]),
        Err(MockError::DuplicateSession(first.attempt))
    );
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
    let second = scheduler.claim_ready().pop().unwrap();
    let second_session = mock
        .start(second.clone(), vec![Step::Finish(Outcome::ResultReported)])
        .unwrap();
    assert_ne!(first_session, second_session);
    assert_eq!(mock.tick(&mut scheduler).unwrap(), vec![second.attempt]);
    assert!(!scheduler.all_completed());
    assert!(scheduler.verify(second.attempt, Verification::Passed));
    assert!(scheduler.all_completed());
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
}

#[test]
fn scripted_crash_and_cancellation_do_not_report_success() {
    let mut scheduler = scheduler();
    let mut mock = MockBackend::default();
    let first = scheduler.claim_ready().pop().unwrap();
    mock.start(first, vec![Step::Finish(Outcome::Crashed)])
        .unwrap();
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
    assert_eq!(
        scheduler.state("work").unwrap(),
        TaskState::Interrupted {
            cancellation_requested: false
        }
    );
    assert!(scheduler.claim_ready().is_empty());
    scheduler.reconcile_for_retry("work").unwrap();

    let second = scheduler.claim_ready().pop().unwrap();
    mock.start(second, vec![Step::Finish(Outcome::ResultReported)])
        .unwrap();
    assert!(scheduler.cancel("work").unwrap());
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
    assert_eq!(scheduler.state("work").unwrap(), TaskState::Cancelled);
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
}

#[test]
fn cancelling_a_blocked_requester_does_not_stop_its_peer() {
    let tasks = ["helper", "requester"].map(|id| PlannedTask {
        graph: TaskSpec {
            id: id.into(),
            depends_on: vec![],
        },
        role: Role::Developer,
        priority: 0,
        allowed_paths: vec![],
        acceptance: vec!["Mock check".into()],
    });
    let plan = Plan {
        version: PLAN_VERSION,
        tasks: tasks.into(),
    }
    .validate(
        Limits {
            max_tasks: 2,
            max_depth: 1,
            max_parallel: 2,
            max_attempts: 1,
        },
        &[],
    )
    .unwrap();
    let agents = ["ada", "lin"].map(|id| Agent {
        id: id.into(),
        role: Role::Developer,
    });
    let mut scheduler = Scheduler::new(plan, agents.into()).unwrap();
    let mut mock = MockBackend::default();
    let claims = scheduler.claim_ready();
    mock.start(
        claims[0].clone(),
        vec![
            Step::Yield,
            Step::Answer {
                body: "answer".into(),
            },
            Step::Finish(Outcome::ResultReported),
        ],
    )
    .unwrap();
    mock.start(
        claims[1].clone(),
        vec![Step::Ask {
            task: "helper".into(),
            body: "question".into(),
        }],
    )
    .unwrap();

    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
    assert_eq!(scheduler.state("requester").unwrap(), TaskState::Blocked);
    assert!(scheduler.cancel("requester").unwrap());
    assert!(mock.tick(&mut scheduler).unwrap().is_empty());
    assert_eq!(scheduler.state("requester").unwrap(), TaskState::Cancelled);
    assert_eq!(mock.tick(&mut scheduler).unwrap(), vec![claims[0].attempt]);
    assert!(scheduler.verify(claims[0].attempt, Verification::Passed));
}
