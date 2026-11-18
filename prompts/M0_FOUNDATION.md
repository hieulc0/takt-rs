# Milestone M0 — First Implementation Assignment

**Priority:** build the smallest *real* deterministic orchestration slice offline. No cloud credentials, databases, networking, or real agent processes.

## Allowed scope

- Inspect existing `src/domain/dag.rs` scaffold and improve types if needed.
- Implement versioned `Plan` with tasks, roles, depends_on, allowed_paths, acceptance criteria, configured limits.
- DAG validator checks duplicate IDs, missing refs, self-ref, cycles, scope safety, max nodes/depth, mandatory acceptance criteria.
- `MockBackend` with deterministic scripted events, including success, help, failure, cancel, and crash.
- In-memory scheduler owns role slots, claims READY tasks exactly once, drives dependencies only after a verified mocked completion.
- Event router stores typed in-memory event list and correlates help message/answer. Bound loop/retry count.
- CLI `takt demo` prints a clearly labeled simulated workflow with two dev tasks, one blocker/help request, verifier stage, and pending approval; `takt doctor` prints actually implemented capabilities.
- Unit tests that prove each invariant, including restart/crash state modeling (not actual persistence yet).

## Required acceptance

```text
A. Invalid DAG with cycle/duplicate/missing dependency is rejected.
B. A and B can be ready concurrently when independent.
C. C depending on A+B does not run until both have verified completion.
D. With max_parallel=1, no second worker starts while slot occupied.
E. Duplicate completion event does not complete twice.
F. Mock help request is routed and recorded, not mistaken for completion.
G. Crash event transitions attempt to INTERRUPTED, not COMPLETED.
H. Demo explains that merge approval is simulated, never performs Git merge.
```

## Exclusions

No SQLite, Tokio daemon, provider CLI spawn, real worktree creation, Telegram polling, native session resume, git push, deployment, OS sandbox, or user secrets in this milestone.

## Output expected from coding agent

- Small plan, implemented file names, meaningful commits if repo is initialized.
- Exact commands and exit codes for fmt, clippy and tests.
- Short demo output showing full simulated event trace.
- List of unimplemented future milestones.
- One clean recommendation for M1 and any focused ADR corrections.
