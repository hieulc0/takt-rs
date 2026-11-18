use takt_rs::{
    domain::{
        dag::TaskSpec,
        event::{Event, MessageKind, Outcome, Verification},
        plan::{Agent, Limits, Plan, PlannedTask, Role, PLAN_VERSION},
        scheduler::Scheduler,
    },
    mock::{MockBackend, Step},
};

pub fn run() -> Result<Vec<Event>, String> {
    let plan = Plan {
        version: PLAN_VERSION,
        tasks: vec![
            task(
                "contract",
                Role::Developer,
                &[],
                "Publish the response contract",
            ),
            task(
                "client",
                Role::Developer,
                &[],
                "Use the agreed response field",
            ),
            task(
                "integration-check",
                Role::Verifier,
                &["contract", "client"],
                "Check both sides of the contract",
            ),
        ],
    };
    let limits = Limits {
        max_tasks: 3,
        max_depth: 2,
        max_parallel: 2,
        max_attempts: 2,
    };
    let agents = [
        ("dev-ada", Role::Developer),
        ("dev-lin", Role::Developer),
        ("verifier", Role::Verifier),
    ]
    .map(|(id, role)| Agent {
        id: id.into(),
        role,
    });
    let plan = plan
        .validate(limits, &["src"])
        .map_err(|error| format!("{error:?}"))?;
    let mut scheduler =
        Scheduler::new(plan, agents.into()).map_err(|error| format!("{error:?}"))?;
    let mut mock = MockBackend::default();
    for _ in 0..16 {
        for assignment in scheduler.claim_ready() {
            let script = script(&assignment.task)?;
            mock.start(assignment, script)
                .map_err(|error| format!("{error:?}"))?;
        }
        for attempt in mock
            .tick(&mut scheduler)
            .map_err(|error| format!("{error:?}"))?
        {
            // The demo fixture supplies the verifier decision outside the provider.
            scheduler.verify(attempt, Verification::Passed);
        }
        if scheduler.all_completed() {
            let mut events = scheduler.events().to_vec();
            events.push(Event::SimulatedApprovalPending);
            return Ok(events);
        }
    }
    Err("workflow did not finish within 16 simulated ticks".into())
}

fn task(id: &str, role: Role, depends_on: &[&str], acceptance: &str) -> PlannedTask {
    PlannedTask {
        graph: TaskSpec {
            id: id.into(),
            depends_on: depends_on
                .iter()
                .map(|dependency| (*dependency).into())
                .collect(),
        },
        role,
        priority: 0,
        allowed_paths: if role == Role::Developer {
            vec![format!("src/{id}.rs")]
        } else {
            vec![]
        },
        acceptance: vec![acceptance.into()],
    }
}

fn script(task: &str) -> Result<Vec<Step>, String> {
    let success = Step::Finish(Outcome::ResultReported);
    match task {
        "contract" => Ok(vec![
            Step::Yield,
            Step::Answer {
                body: "Use the display_name field.".into(),
            },
            success,
        ]),
        "client" => Ok(vec![
            Step::Ask {
                task: "contract".into(),
                body: "Which response field should the client use?".into(),
            },
            success,
        ]),
        "integration-check" => Ok(vec![success]),
        _ => Err(format!("missing demo script for {task}")),
    }
}

pub fn describe(event: &Event) -> String {
    match event {
        Event::Ready(task) => format!("{task} ready"),
        Event::Claimed(assignment) => format!(
            "{} assigned to {} (attempt {})",
            assignment.task, assignment.agent, assignment.attempt.0
        ),
        Event::StateChanged { task, state } => format!("{task} → {state:?}"),
        Event::ProviderOutcome { attempt, outcome } => {
            format!("mock attempt {} reported {outcome:?}", attempt.0)
        }
        Event::Verified { attempt, result } => format!(
            "simulated verification for attempt {} → {result:?}",
            attempt.0
        ),
        Event::MessageQueued(message) => {
            let kind = match message.kind {
                MessageKind::HelpRequest => "help request".into(),
                MessageKind::HelpAnswer { request } => format!("answer to message {}", request.0),
            };
            format!(
                "queued message {} ({kind}) from attempt {} to {} — {}",
                message.id.0, message.from.0, message.to.0, message.body
            )
        }
        Event::MessageDelivered { message, to } => {
            format!("delivered message {} to attempt {}", message.0, to.0)
        }
        Event::RetryScheduled(task) => format!("{task} scheduled for retry"),
        Event::SimulatedApprovalPending => {
            "simulated merge approval pending; no Git action performed".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_repeatable_and_ends_at_the_simulated_gate() {
        let events = run().unwrap();
        assert_eq!(events, run().unwrap());
        assert_eq!(events.last(), Some(&Event::SimulatedApprovalPending));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, Event::MessageDelivered { .. }))
                .count(),
            2
        );
        let verifier = events
            .iter()
            .position(|event| {
                matches!(event,
            Event::Claimed(assignment) if assignment.task == "integration-check")
            })
            .unwrap();
        assert_eq!(
            events[..verifier]
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Verified {
                        result: Verification::Passed,
                        ..
                    }
                ))
                .count(),
            2
        );
    }
}
