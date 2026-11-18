# Product Requirements — MVP

## Personas and interfaces

**Owner:** single trusted local OS user. CLI and TUI speak to local daemon over a Unix-domain socket. Optional Telegram private bot, independently authorized by numeric sender ID and private chat ID, speaks through the same command service.

**Agent:** a logical role backed by an approved provider adapter. Can be enabled, disabled, busy, idle, unavailable, or recovering. Agent names/descriptions remain flexible; access is controlled separately from personality.

## User stories (P0 unless noted)

| ID | Story | Observable acceptance |
|---|---|---|
| U01 | Register a project | Path is canonicalized, must be a Git worktree, baseline commit recorded |
| U02 | Submit request | Workflow persisted and visible after restart |
| U03 | Produce plan | Lead returns machine-validated DAG, scopes, dependencies, and tests |
| U04 | Approve plan | A pending plan cannot dispatch protected or destructive work without approval |
| U05 | Assign agents | Each ready task is claimed at most once; one session turn in flight per agent session |
| U06 | Concurrent code | Two tasks use separate worktrees based on identical baseline |
| U07 | Help request | Coordinator persists, routes, and correlates answer; recipient continues later |
| U08 | Pause/resume | Pause prevents new dispatch; optional cancel-running is separate explicit action |
| U09 | Cancel | Stops active turns/process trees where supported and reaches terminal cancelled state |
| U10 | Verify | Test commands run from an allowlisted verifier policy; capture exit code + output digest |
| U11 | Review | Reviewer receives diff and evidence; approval not accepted on failed mandatory checks |
| U12 | Merge | Approval binds exact target ref and expected HEAD; CAS prevents TOCTOU merge |
| U13 | Recover | Crash -> reconcile DB claims/sessions, don't assume completion or rerun unknown commands |
| U14 | Telegram | Authorized private chat can submit/status/pause; rejects unauthorized identities |
| U15 | Audit | Workflow/task/session/approval status has durable event sequence |
| U16 | Terminal UI | Observe status and logs; advanced TUI is P1 |

## Functional requirements

### Planning
- Capture request, repo, baseline SHA, user-defined constraints, maximum work scope, and requested agents.
- Lead emits structured JSON; parser rejects unexpected versions or unbounded plans.
- Every task: stable ID, role, instruction, dependencies, permitted paths, acceptance evidence, optional expected outputs.
- Maximum 12 tasks/plan, 2 developer tasks concurrently by default, 2 replans/workflow, bounded depth. These are configurable policies, not provider prompts.
- On poor plan: explanatory `PLAN_REJECTED` event; lead may repair within a strict attempt limit.

### Execution
- Worker assignment is atomic in SQLite; lease/attempt token identifies a particular execution.
- Worktree created per coding task from immutable baseline. Only integration step may combine commits.
- Native provider session ID stored independently of internal session ID.
- Structured result includes status, commit(s), changed files, checks requested, blockers, and evidence references; verify against observed Git state.
- Provider crashes, timeouts, and rate limits are distinct failure types; never treat auth/quota failures as transient retryable errors.

### Coordination
- Durable events: task/plan/agent/session/approval/message changes.
- Agent messages target task/role/session and include correlation ID and bounded payload.
- If recipient busy, hold pending delivery; don't inject text into active turn blindly.
- Help loops have hop count and escalation budget; no autonomous infinite group chat.

### Verification and integration
- Run configured commands (argv array, not shell strings) in restricted workspace; verify exit status and output digest.
- Build integration branch from recorded baseline; apply commits; if conflict, enter BLOCKED with explicit resolution request.
- Independent reviewer reads actual diff plus checks and acceptance criteria.
- Approval is valid for exact target ref + expected head + diff digest + expiry, not merely task ID.

### Telegram
- Private, allowlisted user + chat; no groups by default.
- Restrict maximum message length, output verbosity, concurrent requests, and approval lifetime.
- Delete or redact secrets from logs and outbound messages; no arbitrary `exec` endpoint.
- CLI and Telegram receive events from same persisted source; UI cannot become source of truth.

## Nonfunctional requirements

- **Footprint:** single daemon + SQLite; no external infrastructure; target idle baseline under 50 MiB excluding child providers (measure, do not promise).
- **Responsiveness:** status queries should be near-instant locally; model latency excluded.
- **Reliability:** durable state before emitting event externally; reconcile interrupted attempts.
- **Security:** fail closed; no default YOLO; protect socket and secrets; OS sandbox when enforcing filesystem limits.
- **Portability:** Linux first; other OS adapters later.
- **Observability:** structured logs with correlation IDs; status includes last authoritative event sequence.
- **Compatibility:** supported provider APIs, protocol-version checks, no private endpoints.

## Scope boundary and release gate

M0 mock+DAG; M1 daemon+SQLite+IPC; M2 Codex native sessions; M3 agent orchestration+worktrees+verification; M4 Telegram; M5 TUI and hardening. No provider credentials required until M2. No remote deployment or internet-exposed daemon in MVP.
