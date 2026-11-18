# Provider Adapters and Native Sessions

## Design principle

A provider adapter may call **only official, documented interfaces with permitted credentials**. Takt orchestrates, but does not harvest provider cookies/tokens or impersonate subscription clients. Support is conditional on provider policy and the specific authentication method. Compliance should be reviewed before enabling unattended operation.

## MVP adapter order

1. **MockBackend** (no external process, deterministic events and injected failures). Required for scheduler/recovery tests.
2. **Codex App Server** via local stdio, documented bidirectional JSON-RPC-like messages. Recommended for streaming, approvals, and native thread continuity.
3. Optional **Codex exec** mode for bounded one-shot tasks using JSONL and explicit native session resume.
4. Later: Gemini CLI headless mode / approved Gemini API adapter. Validate accepted authentication path; do not claim unsupported resume semantics.
5. Antigravity, Claude consumer subscription, or other connectors only after explicit support and terms review. No account rotation or YOLO shortcuts.

## Normalized adapter contract (design, not implemented)

```rust
#[async_trait::async_trait]
pub trait AgentBackend: Send + Sync {
    fn capabilities(&self) -> BackendCapabilities;
    async fn start_session(&self, req: StartSession) -> Result<SessionHandle, BackendError>;
    async fn send_turn(&self, session: &SessionHandle, req: TurnRequest) -> Result<TurnHandle, BackendError>;
    async fn resume_session(&self, session: &StoredSession) -> Result<SessionHandle, BackendError>;
    async fn cancel_turn(&self, turn: &TurnHandle) -> Result<(), BackendError>;
    async fn poll_events(&self, session: &SessionHandle) -> Result<Vec<ProviderEvent>, BackendError>;
}
```

An implementation may prefer a push/event-stream interface; the facade should normalize that to durable events. `resume` can return `Unsupported`, not silently create a new session. Capability flags: `native_resume`, `streaming`, `turn_cancel`, `approval_requests`, `structured_output`, `file_editing`, `session_reuse`.

## Native Codex protocol guidance

Official documentation describes `codex app-server` over **stdio** (default) using one JSON message per line; JSON-RPC 2.0 semantics with `jsonrpc` omitted on the wire. Typical lifecycle: initialize, thread/start, turn/start; stream item and turn events; answer server-initiated approval requests; thread/read or thread/resume as documented by installed protocol version. **Generate a current JSON schema from the installed Codex binary**, or use published schema; don't hard-code an outdated message shape from these docs.

- Start via `tokio::process::Command::new("codex").arg("app-server")`, no shell.
- Reserve one app-server connection/session coordinator per managed instance; preserve stdout exclusively for protocol, logs on stderr.
- Match request IDs to responses and process unsolicited notifications; enforce bounded JSONL frame size.
- Native `thread_id` stored in SQLite, scoped to provider + workspace + policy profile.
- Policy: default guarded approvals and sandbox. Do not pass `--dangerously-bypass-approvals-and-sandbox`.
- Forward approval requests to the central policy engine; never accept solely because the provider requested permission.
- Track terminal events (`turn/completed`, `turn/interrupted`, `turn/failed`, as supported by version) and unknown event types safely.
- Lost stdio process -> session RECONCILING, attempt INTERRUPTED if tool outcome uncertain.

Official reference: https://learn.chatgpt.com/docs/app-server

## Fallback Codex exec

The official Codex CLI supports `codex exec --json` streaming and `codex exec resume <SESSION_ID>`. Its JSONL output can be normalized into events; use `--output-schema` when applicable to constrain model result. For one-shot execution, stdin prompt avoids exposing long text in process arguments. Manage stdout/stderr asynchronously with limits; fail if result lacks observed evidence. Do not confuse JSONL event stream with a durable native thread by itself.

Official reference: https://learn.chatgpt.com/docs/developer-commands

## Gemini, later

Gemini CLI documents headless mode with `--output-format stream-json` and session resume through `--resume`; event stream includes init, message, tool_use, tool_result, error, result. Implement only after validating installed-version flags and approved credentials. Avoid consumer-account rotation or treating service quotas as an incentive to switch accounts.

Official references: https://geminicli.com/docs/cli/headless/ and https://geminicli.com/docs/cli/session-management/

## Session and task binding

- One native session can have many sequential turns for one logical agent in one project/workspace.
- Sessions are not globally shared across worktrees; a new task in a different worktree requires a different native context unless the provider supports an explicit safe fork.
- Explicit `context_handoff` artifact (summary + pointers) handles cross-agent context. It is not a native resume.
- Persistent session != running process. After reboot, only native history that the provider retained can be resumed.

## Error taxonomy

`UnsupportedCapability`, `AuthRequired`, `QuotaExceeded`, `PolicyRejected`, `TransientTransport`, `ProviderFailure`, `TurnCancelled`, `ProcessExited`, `MalformedStream`, `UnknownOutcome`. Scheduler policy must distinguish all of these.

## Result envelope

```json
{
  "version": 1,
  "task_id": "oauth-backend",
  "attempt_id": "attempt-002",
  "status": "submitted_for_verification",
  "summary": "Added callback endpoint and session cookie handling",
  "commit_sha": "<actual commit sha>",
  "changed_files": ["src/auth/callback.rs"],
  "claimed_checks": ["cargo test auth"],
  "blockers": [],
  "artifacts": []
}
```

This is agent-provided data, not trust. Rust independently checks paths/commit state, then performs authorized verification. Never merge based only on this envelope.
