//! Offline CLI for the M0 mock workflow.
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
            println!("Takt M0 (offline)");
            println!("Available: validated plans, role-based scheduling, scripted mock sessions");
            println!(
                "Available: in-memory event trace, help routing, retry, cancellation, interruption"
            );
            println!("Run the simulated workflow with takt demo.");
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
            eprintln!("Unknown command. Run takt --help.");
            return ExitCode::FAILURE;
        }
    }
    ExitCode::SUCCESS
}

fn print_help() {
    println!("Takt — offline orchestration foundation");
    println!("Usage: takt [--help | --version | doctor | demo]");
}
