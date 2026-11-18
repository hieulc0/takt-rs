# Test Plan and End-to-End Acceptance

## Layers

**Unit (offline):** DAG validation, state machine, scheduler role slots, message IDs, approval checks, path normalization. No providers, Telegram, or SQLite needed for M0.

**Integration (offline):** SQLite migrations, transaction/outbox, Unix socket IPC, MockBackend event stream, simulated daemon crash, Git in temporary repos, simulated Telegram updates.

**Contract (version-aware):** Recorded Codex App Server request/response fixtures and schema compatibility; live smoke only opt-in, never in default CI.

**End-to-end (opt-in):** Disposable Git repo, real Codex agent, verified diff, human approval, Telegram private bot.

## Required acceptance cases

| ID | Scenario | Pass condition |
|---|---|---|
| A01 | DAG cycle A->B->A | Reject before persist/dispatch |
| A02 | Missing dependency ID | Reject with actionable error |
| A03 | Duplicate task ID | Reject |
| A04 | Two ready tasks, two agent slots | Both can run; no same worktree |
| A05 | One slot, two tasks | Exactly one active |
| A06 | Task B depends on A | B not ready until verified A completion |
| A07 | Double scheduler wakeup | One active attempt for task |
| A08 | Crash after outbox committed, before dispatch ack | UNKNOWN/INTERRUPTED until reconcile; not blindly replayed |
| A09 | Duplicate terminal provider event | One terminal state and completion event |
| A10 | Help request while target agent busy | Persist pending message, deliver on next turn, no mid-turn injection |
| A11 | Agent declares success but uncommitted Git changes | Verification fails |
| A12 | Test exit != 0 | No approval created, rework/block |
| A13 | Changed integrated commit SHA | Prior check evidence invalidated |
| A14 | Git worktree dirty before cleanup | Preserve worktree; require explicit cleanup |
| A15 | Untrusted symlink/path traversal | Reject writes/reads outside allowed root (OS sandbox for enforceable isolation) |
| A16 | Provider quota exhausted | Provider blocked; no rotation/retry storm |
| A17 | Child process ignores SIGTERM | Kill process group after grace period |
| A18 | Unauthorized Telegram sender/chat | No information disclosure or state change |
| A19 | Duplicate Telegram update | At most one submitted workflow/action |
| A20 | Expired or reused approval nonce | Reject |
| A21 | Expected target HEAD changed | Abort merge, require new approval |
| A22 | Telegram offline | Local CLI continues; queued notifications bounded |
| A23 | Restart daemon | Task/session records readable and reconciliation performed |
| A24 | Max replan/hop budget | Escalate to human; no infinite agent loop |

## Deterministic mock scenarios

```text
Scenario 1: PLANNED -> dev A + dev B -> tests -> review -> await approve -> complete
Scenario 2: dev B HELP_REQUEST -> mailbox -> dev A answer -> dev B continues
Scenario 3: provider crashes mid tool -> INTERRUPTED -> manual reconciliation
Scenario 4: unknown external dispatch after DB crash -> neither double-exec nor false completion
Scenario 5: stale approval -> FAILED_PRECONDITION without changing branch
```

## Evidence format

Each test report includes command, start time, duration, commit SHA (when applicable), exit code, stdout/stderr truncated log reference, environment/sandbox profile, and reporter. Claims from a model don't count as independent evidence.

## Performance checks

Measure (do not assume): idle RSS, daemon startup, SQLite p95 query latency, status IPC latency, event replay for 10k events, and concurrent mock tasks. Child agent processes excluded from daemon baseline. Recording instrumentation should itself be bounded.
