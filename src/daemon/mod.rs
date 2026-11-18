//! Linux single-writer mock daemon and its local command client.
mod client;
mod engine;
mod paths;
mod protocol;
mod store;

pub use client::run as run_cli;

use std::path::Path;
use std::process::Command as Process;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use engine::{Boundary, Engine};
use paths::Paths;
use protocol::{Command, Request, Response, VERSION};
use store::Store;
use takt_rs::domain::event::AttemptId;

type Error = Box<dyn std::error::Error>;
type Result<T> = std::result::Result<T, Error>;

fn error(message: impl Into<String>) -> Error {
    message.into().into()
}

const POLL: Duration = Duration::from_millis(10);
static SIGNAL_STOP: AtomicBool = AtomicBool::new(false);

pub fn run(paths: &Paths, mut hook: impl FnMut(Boundary, Option<AttemptId>)) -> Result<()> {
    install_signals()?;
    let socket = paths
        .bind()
        .map_err(|err| error(format!("cannot bind daemon socket: {err}")))?;
    let mut store = Store::open(&paths.database())?;
    store.recover()?;
    let mut engine = Engine::new(store);
    while !SIGNAL_STOP.load(Ordering::Relaxed) {
        match socket.listener.accept() {
            Ok((mut stream, _)) => {
                if paths::check_peer(&stream).is_ok() && protocol::configure(&stream).is_ok() {
                    let reply = match protocol::read::<Request>(&stream) {
                        Ok(request) if request.version == VERSION => {
                            handle(&mut engine.store, request.command)
                        }
                        Ok(_) => Err(error("unsupported IPC protocol version")),
                        Err(err) => Err(err.into()),
                    };
                    let response = reply.unwrap_or_else(|err| Response::Error {
                        message: err.to_string(),
                    });
                    let _ = protocol::write(&mut stream, &response);
                    if matches!(response, Response::Stopping) {
                        break;
                    }
                }
            }
            Err(err)
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) => {}
            Err(err) => return Err(err.into()),
        }
        engine.advance(&mut hook)?;
        thread::sleep(POLL);
    }
    engine.finish()
}

fn handle(store: &mut Store, command: Command) -> Result<Response> {
    match command {
        Command::Ping => Ok(Response::Pong),
        Command::Stop => Ok(Response::Stopping),
        Command::ProjectAdd { id, root } => {
            if id.is_empty()
                || id.len() > 64
                || !id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
            {
                return Err(error(
                    "project ID must have 1–64 letters, digits, dashes or underscores",
                ));
            }
            let root = Path::new(&root).canonicalize()?;
            let git_root = git(&root, &["--show-toplevel"])?;
            if Path::new(&git_root).canonicalize()? != root {
                return Err(error("project path must be a Git working tree root"));
            }
            let root = root
                .to_str()
                .ok_or_else(|| error("project path must be UTF-8"))?;
            store.add_project(&id, root)?;
            Ok(Response::ProjectAdded { id })
        }
        Command::WorkflowSubmit { project, request } => {
            if request.trim().is_empty()
                || request.len() > 4096
                || request.chars().any(char::is_control)
            {
                return Err(error(
                    "workflow request must be a single line of 1–4096 bytes",
                ));
            }
            let root = store.project_root(&project)?;
            let baseline = git(Path::new(&root), &["--verify", "HEAD^{commit}"])?;
            let mut scheduler = crate::demo::scheduler().map_err(error)?;
            let id = store.submit(&project, &request, &baseline, &mut scheduler)?;
            Ok(Response::Submitted { id })
        }
        Command::WorkflowStatus { id } => Ok(Response::Workflow(store.workflow(id)?)),
        Command::Events { id, after } if after >= 0 => store.events(id, after),
        Command::Events { .. } => Err(error("event sequence must be non-negative")),
    }
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let result = Process::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .args(arguments)
        .output()?;
    if !result.status.success() {
        return Err(error(
            "Git could not resolve the requested repository or HEAD",
        ));
    }
    Ok(String::from_utf8(result.stdout)?
        .trim_end_matches('\n')
        .into())
}

extern "C" fn stop_signal(_: libc::c_int) {
    SIGNAL_STOP.store(true, Ordering::Relaxed);
}

fn install_signals() -> Result<()> {
    // SAFETY: zero-initialized sigaction is filled with a valid handler and mask
    // before sigaction reads it. The handler only stores a lock-free atomic bool.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = stop_signal as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGINT, libc::SIGTERM] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
