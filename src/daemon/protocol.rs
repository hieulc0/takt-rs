use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use takt_rs::domain::event::{Event, TaskState};

pub const VERSION: u32 = 1;
const MAX_FRAME: usize = 65_536;
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub version: u32,
    pub command: Command,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Stop,
    ProjectAdd { id: String, root: String },
    WorkflowSubmit { project: String, request: String },
    WorkflowStatus { id: i64 },
    Events { id: i64, after: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    Interrupted,
    Failed,
    AwaitingApproval,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
            Self::AwaitingApproval => "awaiting_approval",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Workflow {
    pub id: i64,
    pub project: String,
    pub request: String,
    pub baseline: String,
    pub status: Status,
    pub tasks: Vec<(String, TaskState)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Entry {
    pub seq: i64,
    pub event: Event,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum Response {
    Pong,
    Stopping,
    ProjectAdded {
        id: String,
    },
    Submitted {
        id: i64,
    },
    Workflow(Workflow),
    Events {
        entries: Vec<Entry>,
        pruned_through: i64,
    },
    Error {
        message: String,
    },
}

pub fn configure(stream: &UnixStream) -> io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))
}

pub fn read<T: DeserializeOwned>(stream: &UnixStream) -> io::Result<T> {
    let deadline = Instant::now() + TIMEOUT;
    let mut input = Vec::new();
    let mut buffer = [0; 4096];
    let mut stream = stream;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "JSON frame deadline elapsed",
            ));
        }
        stream.set_read_timeout(Some(remaining))?;
        let size = stream.read(&mut buffer)?;
        if size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete JSON frame",
            ));
        }
        let end = buffer[..size].iter().position(|&byte| byte == b'\n');
        let content = end.map_or(size, |index| index + 1);
        if input.len() + content > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized JSON frame",
            ));
        }
        input.extend_from_slice(&buffer[..content]);
        if end.is_some() {
            return serde_json::from_slice(&input).map_err(io::Error::other);
        }
    }
}

pub fn write(stream: &mut UnixStream, value: &impl Serialize) -> io::Result<()> {
    let mut output = serde_json::to_vec(value).map_err(io::Error::other)?;
    output.push(b'\n');
    if output.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "oversized JSON frame",
        ));
    }
    stream.write_all(&output)
}
