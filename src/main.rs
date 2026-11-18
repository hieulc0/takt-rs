//! Takt CLI and local daemon.
#[cfg(target_os = "linux")]
mod daemon;
mod demo;

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.as_slice() {
        [] => print_help(),
        [arg] if arg == "--help" || arg == "-h" => print_help(),
        [arg] if arg == "--version" || arg == "-V" => {
            println!("takt {}", env!("CARGO_PKG_VERSION"));
        }
        [arg] if arg == "doctor" => {
            println!("Takt mock orchestration");
            println!("Available: validated plans, role-based scheduling, scripted mock sessions");
            println!(
                "Available: in-memory event trace, help routing, retry, cancellation, interruption"
            );
            #[cfg(target_os = "linux")]
            println!("Available: SQLite state and local Unix socket IPC. Run takt daemon status to check the daemon.");
        }
        [arg] if arg == "demo" => match demo::run() {
            Ok(events) => {
                println!("Takt M0 demo — simulated provider, checks and merge approval");
                for (index, event) in events.iter().enumerate() {
                    println!("{:02} {}", index + 1, demo::describe(event));
                }
            }
            Err(error) => {
                eprintln!("Demo failed: {error}");
                return ExitCode::FAILURE;
            }
        },
        _ => {
            if let Err(error) = run_daemon_cli(&args) {
                eprintln!("takt: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

#[cfg(target_os = "linux")]
use daemon::run_cli as run_daemon_cli;

#[cfg(not(target_os = "linux"))]
fn run_daemon_cli(_args: &[String]) -> Result<(), &'static str> {
    Err("the M1 daemon requires Linux")
}

fn print_help() {
    println!("Takt — local mock orchestration");
    println!("Usage: takt [--help | --version | doctor | demo]");
    #[cfg(target_os = "linux")]
    print_daemon_help();
}

#[cfg(target_os = "linux")]
fn print_daemon_help() {
    println!("       takt daemon [start | status | stop | run]");
    println!("       takt project add <id> <git-root>");
    println!("       takt workflow submit <project> '<request>'");
    println!("       takt workflow status <id>");
    println!("       takt events tail <id> [--after <seq>] [--follow]");
}
