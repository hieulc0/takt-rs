use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command as Process, Stdio};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use takt_rs::domain::event::{Event, TaskState};
use tempfile::TempDir;

use super::{
    client::call,
    engine::Boundary,
    paths::Paths,
    protocol::{Command, Response, Status},
    store::Store,
};

const CHILD_ROOT: &str = "TAKT_TEST_CHILD_ROOT";
const CHILD_BOUNDARY: &str = "TAKT_TEST_CHILD_BOUNDARY";
const CHILD_OCCURRENCE: &str = "TAKT_TEST_CHILD_OCCURRENCE";

#[test]
fn daemon_subprocess() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let root = Path::new(&root);
    let boundary = std::env::var(CHILD_BOUNDARY).unwrap_or_default();
    let mut occurrence = std::env::var(CHILD_OCCURRENCE)
        .unwrap()
        .parse::<usize>()
        .unwrap();
    super::run(&Paths::test(root), |current, attempt| {
        if current == Boundary::Started {
            let mut audit = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("dispatch-audit"))
                .unwrap();
            writeln!(audit, "{}", attempt.unwrap().0).unwrap();
            audit.sync_all().unwrap();
        }
        if format!("{current:?}") == boundary {
            occurrence -= 1;
            if occurrence == 0 {
                fs::write(root.join("parked"), b"ready to kill").unwrap();
                loop {
                    std::thread::park();
                }
            }
        }
    })
    .unwrap();
}

struct Daemon(Child);

impl Daemon {
    fn spawn(root: &Path, boundary: Option<Boundary>, occurrence: usize) -> Self {
        let child = Process::new(std::env::current_exe().unwrap())
            .args(["--exact", "daemon::tests::daemon_subprocess", "--nocapture"])
            .env(CHILD_ROOT, root)
            .env(
                CHILD_BOUNDARY,
                boundary
                    .map(|value| format!("{value:?}"))
                    .unwrap_or_default(),
            )
            .env(CHILD_OCCURRENCE, occurrence.to_string())
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        Self(child)
    }

    fn ready(&mut self, paths: &Paths) {
        wait_until(|| {
            assert!(
                self.0.try_wait().unwrap().is_none(),
                "daemon exited before ready"
            );
            matches!(call(paths, Command::Ping), Ok(Response::Pong))
        });
    }

    fn kill(&mut self) {
        self.0.kill().unwrap();
        assert!(!self.0.wait().unwrap().success());
    }

    fn stop(&mut self, paths: &Paths) {
        assert!(matches!(
            call(paths, Command::Stop).unwrap(),
            Response::Stopping
        ));
        wait_until(|| {
            self.0
                .try_wait()
                .unwrap()
                .is_some_and(|status| status.success())
        });
        assert!(!paths.socket().exists());
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn git_project(dir: &TempDir) -> String {
    let root = dir.path().join("repo");
    fs::create_dir(&root).unwrap();
    let run = |args: &[&str]| {
        assert!(Process::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .status()
            .unwrap()
            .success());
    };
    run(&["init", "-q"]);
    run(&[
        "-c",
        "user.name=Takt Test",
        "-c",
        "user.email=takt@example.invalid",
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "--allow-empty",
        "-qm",
        "test baseline",
    ]);
    root.to_str().unwrap().into()
}

fn submit(paths: &Paths, root: String) -> i64 {
    assert!(matches!(
        call(
            paths,
            Command::ProjectAdd {
                id: "test".into(),
                root
            }
        )
        .unwrap(),
        Response::ProjectAdded { .. }
    ));
    let Response::Submitted { id } = call(
        paths,
        Command::WorkflowSubmit {
            project: "test".into(),
            request: "Mock fixture".into(),
        },
    )
    .unwrap() else {
        panic!("unexpected response")
    };
    id
}

fn workflow(paths: &Paths, id: i64) -> super::protocol::Workflow {
    let Response::Workflow(workflow) = call(paths, Command::WorkflowStatus { id }).unwrap() else {
        panic!("unexpected response")
    };
    workflow
}

fn count(db: &Connection, sql: &str) -> i64 {
    db.query_row(sql, [], |row| row.get(0)).unwrap()
}

#[test]
fn sigkill_at_mock_dispatch_and_completion_boundaries() {
    for (boundary, occurrence, sessions, completed, total_attempts) in [
        (Boundary::Claimed, 1, 0, 0, 2),
        (Boundary::Started, 1, 0, 0, 2),
        (Boundary::Acknowledged, 1, 1, 0, 2),
        (Boundary::BeforeCompletionCommit, 1, 2, 0, 2),
        (Boundary::CompletionCommitted, 1, 2, 1, 2),
        (Boundary::CompletionCommitted, 2, 2, 2, 3),
        (Boundary::CompletionCommitted, 3, 3, 3, 3),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::test(dir.path());
        let mut daemon = Daemon::spawn(dir.path(), Some(boundary), occurrence);
        daemon.ready(&paths);
        let id = submit(&paths, git_project(&dir));
        wait_until(|| dir.path().join("parked").exists());
        daemon.kill();

        {
            let db = Connection::open(paths.database()).unwrap();
            assert_eq!(
                count(&db, "SELECT count(*) FROM sessions"),
                sessions,
                "{boundary:?}"
            );
            assert_eq!(
                count(
                    &db,
                    "SELECT count(*) FROM attempts WHERE state = 'completed'"
                ),
                completed
            );
            assert_eq!(count(&db, "SELECT count(*) FROM inbox"), completed);
        }

        let can_continue = completed >= 2;
        let mut restarted = Daemon::spawn(dir.path(), None, 1);
        restarted.ready(&paths);
        let expected = if can_continue {
            Status::AwaitingApproval
        } else {
            Status::Interrupted
        };
        wait_until(|| workflow(&paths, id).status == expected);
        if !can_continue {
            assert!(workflow(&paths, id)
                .tasks
                .iter()
                .any(|(_, state)| matches!(state, TaskState::Interrupted { .. })));
        }
        let Response::Events { entries, .. } =
            call(&paths, Command::Events { id, after: 0 }).unwrap()
        else {
            panic!("unexpected response")
        };
        let first = entries.first().unwrap().seq;
        let Response::Events {
            entries: remainder, ..
        } = call(&paths, Command::Events { id, after: first }).unwrap()
        else {
            panic!("unexpected response")
        };
        assert_eq!(
            entries[1..]
                .iter()
                .map(|entry| entry.seq)
                .collect::<Vec<_>>(),
            remainder.iter().map(|entry| entry.seq).collect::<Vec<_>>()
        );
        assert_eq!(
            entries
                .iter()
                .filter(|entry| matches!(entry.event, Event::Claimed(_)))
                .count() as i64,
            total_attempts
        );

        restarted.stop(&paths);
        let dispatched = fs::read_to_string(dir.path().join("dispatch-audit")).unwrap_or_default();
        let expected_dispatches = match boundary {
            Boundary::Claimed => "",
            Boundary::Started | Boundary::Acknowledged => "1\n",
            _ if can_continue => "1\n2\n3\n",
            _ => "1\n2\n",
        };
        assert_eq!(dispatched, expected_dispatches, "{boundary:?}/{occurrence}");
        let db = Connection::open(paths.database()).unwrap();
        assert_eq!(count(&db, "SELECT count(*) FROM attempts"), total_attempts);
        assert_eq!(
            count(&db, "SELECT count(*) FROM attempts WHERE number > 1"),
            0
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM attempts WHERE state = 'active'"),
            0
        );
        assert_eq!(
            count(
                &db,
                "SELECT count(*) FROM outbox WHERE state IN ('pending', 'acknowledged')"
            ),
            0
        );
        assert_eq!(
            count(&db, "SELECT count(*) FROM pragma_foreign_key_check"),
            0
        );
        if can_continue {
            assert_eq!(count(&db, "SELECT count(*) FROM inbox"), 3);
            assert_eq!(
                count(&db, "SELECT count(*) FROM mailbox WHERE delivered = 1"),
                2
            );
        }
    }
}

#[test]
fn locks_prevent_a_second_daemon_and_permissions_are_private() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::test(dir.path());
    let mut daemon = Daemon::spawn(dir.path(), None, 1);
    daemon.ready(&paths);
    let error = paths.bind().err().expect("second daemon acquired lock");
    assert!(error.to_string().contains("already owns"));
    assert_eq!(
        fs::metadata(paths.socket()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(&paths.state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    daemon.stop(&paths);
}

#[test]
fn failed_transaction_rolls_back_claim_events_and_dispatch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path).unwrap();
    store.add_project("test", "/test").unwrap();
    let mut scheduler = crate::demo::scheduler().unwrap();
    let id = store
        .submit("test", "mock", "base", &mut scheduler)
        .unwrap();
    let db = Connection::open(path).unwrap();
    db.execute_batch(
        "CREATE TRIGGER stop_event BEFORE INSERT ON events
         BEGIN SELECT RAISE(ABORT, 'injected write error'); END;",
    )
    .unwrap();
    assert_eq!(scheduler.claim_ready().len(), 2);
    assert!(store
        .save(id, Status::Running, &mut scheduler, vec![])
        .is_err());
    let stored = store.workflow(id).unwrap();
    assert_eq!(stored.status, Status::Queued);
    assert_eq!(stored.tasks[0].1, TaskState::Ready);
    assert_eq!(count(&db, "SELECT count(*) FROM attempts"), 0);
    assert_eq!(count(&db, "SELECT count(*) FROM outbox"), 0);
    assert_eq!(count(&db, "SELECT count(*) FROM events"), 2);
}

#[test]
fn retention_bounds_event_pages_and_exposes_the_pruned_cursor() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db")).unwrap();
    store.add_project("test", "/test").unwrap();
    let mut scheduler = crate::demo::scheduler().unwrap();
    let id = store
        .submit("test", "mock", "base", &mut scheduler)
        .unwrap();
    let events = vec![Event::SimulatedApprovalPending; 300];
    store
        .save(id, Status::AwaitingApproval, &mut scheduler, events)
        .unwrap();
    let Response::Events {
        entries,
        pruned_through,
    } = store.events(id, 0).unwrap()
    else {
        panic!("unexpected response");
    };
    assert_eq!(pruned_through, 46);
    assert_eq!(entries.len(), 32);
    assert_eq!(entries.first().unwrap().seq, 47);
    let Response::Events { entries, .. } = store.events(id, 300).unwrap() else {
        panic!("unexpected response");
    };
    assert_eq!(entries.len(), 2);
}
