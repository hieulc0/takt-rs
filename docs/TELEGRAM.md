# Telegram Remote Control Specification

## Role

Telegram is an **authenticated input/output adapter** for the local daemon. It may create and inspect workflows, send guidance, pause/cancel work, and approve a precisely identified action. It cannot directly execute host shell commands or bypass the common policy engine.

## Transport

- BotFather token supplied via protected environment file or secret manager (`TAKT_TELEGRAM_TOKEN`), never committed.
- Teloxide long polling first; avoids exposing a public webhook or local port.
- One update poller per bot token. Store update cursor/processed update IDs, reject duplicate actions.
- Exponential backoff on Telegram failures; Telegram outage must not pause local workflows.
- If polling fails continuously, CLI remains functional and reports Telegram degraded.

## Identity and authorization

1. Require numeric allowlisted `from.id` **and** expected private `chat.id`.
2. Reject groups/supergroups/channels and forwarded or anonymous messages for sensitive actions.
3. Require `/pair` performed locally or manually configured numeric IDs; never trust username alone.
4. Map identity to role `owner`; no multi-user ACL in MVP.
5. Rate-limit and enforce maximum request/message size. Sanitize Markdown and avoid printing secrets.
6. Refuse command if user/chat/config not initialized. Explicit auth test must precede all queries, including `/status`.

## Commands (proposal)

| Command | Permissions | Semantics |
|---|---|---|
| `/help` | owner | List available commands |
| `/status` | owner | Health and workflows summary |
| `/agents` | owner | Agent availability and current assignment |
| `/tasks [workflow]` | owner | Paged tasks and state |
| `/run <project> <goal>` | owner | Create workflow, async plan request, return ID |
| `/task <task_id>` | owner | Details, last events, owner, worktree, blockers |
| `/message <task_id> <text>` | owner | Queue guidance via coordinator; never raw PTY typing |
| `/pause <workflow>` | owner | Prevent new dispatch (existing turns continue) |
| `/resume <workflow>` | owner | Resume scheduling if policy allows |
| `/cancel <workflow>` | owner | Intent + cancellation of active turns; show uncertainty |
| `/review <workflow>` | owner | Redacted review summary and diff digest |
| `/approve <approval_id>` | owner | Start two-step approval confirmation for exact action |
| `/reject <approval_id>` | owner | Reject with optional reason |

No `/exec`, `/shell`, `/yolo`, `/rotate-account`, or arbitrary filesystem access.

## Two-step approval

1. Daemon creates approval record with `workflow`, `target_ref`, `expected_head_sha`, `source_sha`, `diff_sha256`, `action`, `expires_at` and random one-time nonce (hash stored in DB).
2. Bot sends short summary (project/branch/count/files/checks/diff digest) and confirmation affordance.
3. Owner confirms through a bot action bound to original approval ID and nonce, with short expiry; verify user+chat again.
4. Daemon transaction atomically changes `PENDING -> CONSUMING` once. Revalidate expected target HEAD/diff/checks; execute merge; write result and audit event.
5. On timeout, repeated action, changed commit, missing checks, or merge conflict -> reject/expire and request fresh review.
6. Do not send credentials, full private code, or large diffs to Telegram by default. Let CLI show full diff locally.

Telegram callbacks are not a security boundary: they are authenticated **inputs** to server-side verification. Never build approval security around opaque callback text alone.

## Messages and delivery

- Truncate long summaries (e.g. first 1,000–2,000 chars) and provide `/task` for detail.
- Queue outbound notifications with dedupe key `(workflow_id,event_seq,recipient)`; occasional duplicates are acceptable, repeated approvals are not.
- Escape untrusted agent-generated Markdown; consider plain text first.
- Send: task blocked, workflow awaiting approval, workflow completed/failed, auth/provider blocked.
- Avoid flooding: aggregate high-frequency progress, rate-limit bot messages, allow mute.

## Private-mode setup sketch

```toml
[telegram]
enabled = false
private_chat_id = 0         # configure actual numeric private chat id
allowed_user_ids = []       # numeric ids, no usernames
notify_on = ["blocked", "approval_requested", "workflow_terminal"]
```

## Tests

- Unauthorized user AND wrong chat rejected; auth checked before any handler.
- Retry of same Telegram update creates at most one workflow.
- Duplicate/expired approval cannot apply twice.
- Changed target HEAD makes approval invalid.
- Secret-like test strings redacted; event output bounded.
- Bot disconnected while daemon continues and CLI stays usable.
