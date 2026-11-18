# Implementation Plan — Small Vertical Slices

## Guiding rule

Implement **one vertical slice at a time**, with real tests and evidence. Do not start Codex, Gemini, and Telegram adapters simultaneously. Never replace an existing working Orbit repo: this is a new project.

## M0 — Scaffold, DAG, mock backend (first coding assignment)

Scope: package structure, basic CLI, pure task graph validator, in-memory scheduler, `MockBackend`, deterministic events, tests. No SQLite/provider calls/network.

Deliverables:
- `src/domain/` task graph validation, dependency readiness, role / scope validation.
- `MockBackend` emits scripted turn events: success, help request, transient failure, crash, cancellation.
- Scheduler is independent of Tokio or infrastructure so it is unit-testable. Queue-ready and claim events captured in memory.
- CLI `takt doctor` and `takt demo` show mocked two-developer workflow; mock demo is clearly labeled.
- Tests: cycle, duplicate ID, unknown dependency, path scope, at-most-once assignment, blocked task unblocking, bounded retries, crash interruption.

**Exit gate:** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` pass; demo prints complete event trace. No real provider credentials.

## M1 — Durable single-writer daemon

- SQLite migration + typed repository layer, WAL, FK, integrity checks; persisted event/outbox/inbox and bounded retention.
- Unix socket server and thin CLI client; ownership lock, startup status, graceful shutdown.
- Transactional transition + event append + outbox dispatch intent.
- Crash recovery scan: reconcile CLAIMED/RUNNING as INTERRUPTED until proven; no duplicate side-effect dispatch.
- `project add`, `workflow submit`, `workflow status`, `events tail`, `daemon start/stop`.

**Exit gate:** kill daemon at each step of mock dispatch; restart has consistent DB and never duplicates completed action. CLI reconnection works.

M1 uses a versioned scheduler checkpoint as the authoritative task state. The
first migration also stores attempts, mock session references, the dispatch
outbox, terminal results in the inbox, addressed mailbox messages, and a
256-event journal per workflow. The daemon runs one workflow at a time and can
assign both developer tasks during that workflow. Submissions run the fixed M0
fixture and record a Git baseline. A pending dispatch from a lost runtime is
conservatively interrupted and is retained for inspection. Real provider
reconciliation and explicit recovery actions are follow-on work.

## M2 — Real Codex adapter

- Spawn `codex app-server` locally over stdio; version-aware initialize and schema-backed parsing.
- Native thread and turn IDs, ordered streaming events, cancellation, provider approval requests, session resume/read.
- Persist session handles; handle partial/unknown outcome after child crash; no blanket permission bypass.
- Validate installed Codex version and official protocol before coding payload shapes.
- `codex exec --json` fallback is optional, not simultaneous requirement.

**Exit gate:** one small opt-in coding task executes in a disposable Git repo; daemon restart can reconcile and resume only where provider supports it. Mock CI remains offline.

## M3 — Hybrid orchestration and Git integration

- Lead task planner generates JSON plan; Rust validates and optionally requests approval.
- Bounded task graph, scheduler+slots, mailbox/help routing, escalation/replan.
- Per-task Git worktrees/branches at baseline SHA, workspace cleanup policy preserving user data.
- Integration worktree, cherry-pick, conflict handling, verifier command registry, reviewer evidence.
- Approval record bound to target/head/digest; atomic merge with optimistic concurrency.

**Exit gate:** two parallel workers, one help message, integration tests, independent review, human-approved merge on disposable demo repo.

## M4 — Telegram remote control

- Teloxide long polling in the daemon process (feature/config gated).
- Exact numeric sender+chat allowlist, private-chat enforcement, dedupe, rate limits.
- Status/agents/tasks/run/message/pause/resume/cancel; redacted notifications.
- Two-step one-time approval with expiry, target HEAD and diff digest recheck.

**Exit gate:** unauthorized/duplicate/replayed/expired actions rejected; bot outage does not stop workflows; CLI sees same state.

## M5 — TUI, hardening, packaging

- Basic Ratatui dashboard reading daemon IPC only; task DAG view, agent/session list, event tail, approval details.
- OS sandbox integration for strong file/network restrictions, process tree termination, resource limits.
- Logs rotation, migrations/backup commands, service unit for user systemd, documentation.
- A second approved provider adapter when justified by use case.

**Exit gate:** clean install instructions, cold restart, interruptions, permission tests, measured idle footprint, no excessive dependencies.

## Development backlog (well-sized issues)

| ID | Milestone | Unit of work | Observable output |
|---|---|---|---|
| TKT-001 | M0 | Normalize domain types and plan schema | Type/unit tests |
| TKT-002 | M0 | DAG validation + readiness | Deterministic test suite |
| TKT-003 | M0 | MockBackend scripted sessions | Event trace tests |
| TKT-004 | M0 | In-memory scheduler & role slots | Parallelism/retry tests |
| TKT-005 | M0 | Demo command | Demonstrated mock workflow |
| TKT-006 | M1 | SQLite migration and store | Migration/integrity tests |
| TKT-007 | M1 | Atomic task claim and outbox | Crash-injection tests |
| TKT-008 | M1 | Unix socket command service | CLI integration tests |
| TKT-009 | M1 | Recovery reconciler | Kill/restart tests |
| TKT-010 | M2 | Codex protocol client | Recorded fixture tests |
| TKT-011 | M2 | Session/turn lifecycle | Native resume/cancel tests |
| TKT-012 | M3 | Planner schema validator | Invalid plan rejected |
| TKT-013 | M3 | Worktrees + Git evidence | No modifications to user tree |
| TKT-014 | M3 | Mailbox/escalation | Correlated delivery test |
| TKT-015 | M3 | Integration + verifier | Stale checks invalidated |
| TKT-016 | M3 | Review and approval record | CAS/expiry/nonce tests |
| TKT-017 | M4 | Telegram auth and polling | Private allowlist tests |
| TKT-018 | M4 | Telegram command routing | Same IPC semantics |
| TKT-019 | M4 | Telegram approval interaction | Replay invalidation tests |
| TKT-020 | M5 | TUI and packaging | Offline docs + smoke test |

## Definition of done for every issue

- Existing working behavior preserved; diff limited to agreed files.
- Tests capture at least one failure before fix for bug tasks.
- New code has unit/integration tests or explicit reason why not.
- All logs and test artifacts are redacted; no tokens in commits.
- `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` results reported with exact commands and exit status.
- Update relevant docs and note actual deviations from planned API/protocol.
- Do not mark implemented simply because interfaces are documented.

## Delivery order to agent

First send [`prompts/START_AGENT.md`](../prompts/START_AGENT.md) plus [`prompts/M0_FOUNDATION.md`](../prompts/M0_FOUNDATION.md). After M0 review, give M1 scope. Avoid telling an agent to 'build the entire app' in one unbounded run.
