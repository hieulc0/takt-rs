use std::env;
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    error,
    paths::Paths,
    protocol::{self, Command, Request, Response, VERSION},
    run as run_daemon, Result,
};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY: Duration = Duration::from_millis(25);

pub fn run(args: &[String]) -> Result<()> {
    let paths = Paths::discover()?;
    match args {
        [cmd, action] if cmd == "daemon" => match action.as_str() {
            "start" => start(&paths),
            "stop" => stop(&paths),
            "status" => {
                call(&paths, Command::Ping)?;
                println!("Takt daemon is running");
                Ok(())
            }
            "run" => run_daemon(&paths, |_, _| {}),
            _ => Err(error("usage: takt daemon [start | status | stop | run]")),
        },
        [cmd, action, id, root] if cmd == "project" && action == "add" => {
            let root = Path::new(root).canonicalize()?;
            let root = root
                .to_str()
                .ok_or_else(|| error("project path must be UTF-8"))?;
            if let Response::ProjectAdded { id } = call(
                &paths,
                Command::ProjectAdd {
                    id: id.clone(),
                    root: root.into(),
                },
            )? {
                println!("Registered project {id}");
            }
            Ok(())
        }
        [cmd, action, project, request] if cmd == "workflow" && action == "submit" => {
            if let Response::Submitted { id } = call(
                &paths,
                Command::WorkflowSubmit {
                    project: project.clone(),
                    request: request.clone(),
                },
            )? {
                println!("{id}");
            }
            Ok(())
        }
        [cmd, action, id] if cmd == "workflow" && action == "status" => {
            if let Response::Workflow(workflow) =
                call(&paths, Command::WorkflowStatus { id: id.parse()? })?
            {
                println!(
                    "Workflow {} [{}] {}",
                    workflow.id,
                    workflow.status.as_str(),
                    workflow.request
                );
                println!("Project {} at {}", workflow.project, workflow.baseline);
                for (task, state) in workflow.tasks {
                    println!("  {task} {state:?}");
                }
            }
            Ok(())
        }
        [cmd, action, id, options @ ..] if cmd == "events" && action == "tail" => {
            let mut after = 0;
            let mut follow = false;
            let mut options = options.iter();
            while let Some(option) = options.next() {
                match option.as_str() {
                    "--after" => {
                        after = options
                            .next()
                            .ok_or_else(|| error("--after requires a sequence"))?
                            .parse()?
                    }
                    "--follow" => follow = true,
                    _ => {
                        return Err(error(
                            "usage: takt events tail <workflow-id> [--after <seq>] [--follow]",
                        ))
                    }
                }
            }
            tail(&paths, id.parse()?, after, follow)
        }
        _ => Err(error("unknown command or arguments. Run takt --help.")),
    }
}

pub(super) fn call(paths: &Paths, command: Command) -> Result<Response> {
    let mut stream = paths.connect()?;
    protocol::configure(&stream)?;
    protocol::write(
        &mut stream,
        &Request {
            version: VERSION,
            command,
        },
    )?;
    match protocol::read(&stream)? {
        Response::Error { message } => Err(error(message)),
        response => Ok(response),
    }
}

fn start(paths: &Paths) -> Result<()> {
    if matches!(call(paths, Command::Ping), Ok(Response::Pong)) {
        println!("Takt daemon is already running");
        return Ok(());
    }
    let mut child = Process::new(env::current_exe()?)
        .args(["daemon", "run"])
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if matches!(call(paths, Command::Ping), Ok(Response::Pong)) {
            println!("Started Takt daemon");
            return Ok(());
        }
        if let Some(status) = child.try_wait()? {
            let mut details = String::new();
            if let Some(stderr) = child.stderr.take() {
                stderr.take(4096).read_to_string(&mut details)?;
            }
            return Err(error(format!(
                "daemon exited with {status}: {}",
                details.trim()
            )));
        }
        thread::sleep(RETRY);
    }
    child.kill()?;
    child.wait()?;
    Err(error("daemon did not become ready"))
}

fn stop(paths: &Paths) -> Result<()> {
    if !matches!(call(paths, Command::Stop)?, Response::Stopping) {
        return Err(error("unexpected daemon response to stop"));
    }
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if !paths.socket().exists() {
            println!("Stopped Takt daemon");
            return Ok(());
        }
        thread::sleep(RETRY);
    }
    Err(error("daemon did not shut down"))
}

fn tail(paths: &Paths, id: i64, mut after: i64, follow: bool) -> Result<()> {
    let mut output = io::stdout().lock();
    let mut disconnected_at: Option<Instant> = None;
    loop {
        let response = match call(paths, Command::Events { id, after }) {
            Ok(response) => {
                disconnected_at = None;
                response
            }
            Err(err)
                if follow
                    && retryable_disconnect(&err)
                    && disconnected_at.get_or_insert_with(Instant::now).elapsed()
                        < START_TIMEOUT =>
            {
                thread::sleep(RETRY);
                continue;
            }
            Err(err) => return Err(err),
        };
        let Response::Events {
            entries,
            pruned_through,
        } = response
        else {
            return Err(error("unexpected daemon response to event request"));
        };
        if after < pruned_through {
            return Err(error(format!("events through sequence {pruned_through} expired; resume with --after {pruned_through}")));
        }
        if entries.is_empty() {
            if !follow {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(250));
        }
        for entry in entries {
            match writeln!(
                output,
                "{} {}",
                entry.seq,
                crate::demo::describe(&entry.event)
            ) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
                Err(err) => return Err(err.into()),
            }
            after = entry.seq;
        }
    }
}

fn retryable_disconnect(err: &super::Error) -> bool {
    err.downcast_ref::<io::Error>().is_some_and(|err| {
        matches!(
            err.kind(),
            io::ErrorKind::NotFound
                | io::ErrorKind::ConnectionRefused
                | io::ErrorKind::ConnectionReset
                | io::ErrorKind::UnexpectedEof
                | io::ErrorKind::BrokenPipe
                | io::ErrorKind::TimedOut
                | io::ErrorKind::WouldBlock
        )
    })
}
