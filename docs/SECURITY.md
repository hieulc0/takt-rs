# Security and Provider Compliance

## Trust zones

1. Owner's local terminal (trusted principal, but command input still validated).
2. Local daemon and SQLite (trusted single-writer service, protected OS permissions).
3. Provider process and external model (not trusted to enforce Takt's access policy).
4. Checked-out repository and task data (potentially untrusted instructions and code).
5. Telegram network/chat (remote input, not a privileged shell).

## Default safety policy

- No `--yolo`, `--dangerously-skip-permissions`, or `--dangerously-bypass-approvals-and-sandbox` defaults.
- No token scraping, importing credential stores, browser cookie reading, or third-party OAuth proxies.
- No account swapping to evade quotas, bans, or provider rate limits.
- Stop on auth/permissions/quota errors and show a clear provider-blocked status.
- No automatic pushes, destructive Git commands, server deployment, or production edits.
- Require explicit approval for merge, privileged filesystem access, network exfiltration, and destructive operations.
- Tool invocations must use argv arrays, avoid shell string interpolation, and set bounded timeouts/output sizes.

## Workspaces and blast radius

- Each code task has unique Git branch and worktree. Verify root canonical path, no `..`, no symlink escape, and clean baseline policy.
- **A worktree does NOT sandbox a process.** A provider can still read home folders unless provider/OS sandbox restricts it.
- Enforcing write allowlists requires provider-native sandbox with demonstrated behavior or an actual OS sandbox (`bubblewrap`/container namespaces). When unavailable, restrict to low-risk tasks and require manual approval.
- Never put service secrets in agent-visible filesystem trees or task prompts; sanitize env passed to subprocesses.
- Git hooks and test scripts are arbitrary code execution. Verification commands need explicit consent/sandbox, and should not automatically run unknown repo scripts with personal credentials in environment.

## Prompt injection / unsafe collaboration

- Repo files, model output, generated plans, and Telegram goal text are **data**, not authority to change runtime policy.
- A task planner cannot add capabilities to itself or ask to enable YOLO through a new task.
- Treat agent output paths, shell snippets, and suggested commands as untrusted; validate against policy.
- Only the command service can authorize merges after nonce/HEAD/digest checks.

## Secrets

- Use OS keyring or owner-readable files with `0600`, folder `0700` for Telegram token / API keys.
- Do not log env vars, `Authorization` headers, request bodies that might contain secrets, or provider native auth tokens.
- SQLite stores provider session IDs and metadata, **not** provider passwords/tokens.
- Secret redactor runs before event persistence and outbound Telegram delivery, but it is defense-in-depth, not a complete guarantee.

## Daemon / IPC

- `XDG_RUNTIME_DIR/takt-rs/` `0700`, socket `0600`; verify Unix peer UID if available.
- Single daemon writer. Lock prevents double daemons. Restrict max request size (e.g. 64 KiB), event stream backpressure, request timeouts.
- Never bind IPC or provider WebSocket listeners to public interfaces in MVP.
- Telegram handler maps requests to explicitly enumerated daemon methods; no raw process/channel access.

## Permissions matrix

| Operation | Background agent | Owner CLI | Authenticated Telegram |
|---|---|---|---|
| View own task artifacts | Scoped | Yes | Redacted summary |
| Request collaboration | Scoped | Yes | Guidance to task |
| Create/approve plan | Suggest only | Yes | Yes if explicit gate |
| Start task | Rust scheduler only | Submit request | Submit request |
| Change provider permission policy | No | Local config+review | No |
| Read unrelated files | No when sandbox enforced | Local OS access | No |
| Merge into main | No | Single-use approval | Single-use approval |
| Push/deploy | No (MVP) | Out of scope | Out of scope |

## Compliance caveat

An official binary is not a universal license for any use. Validate provider service terms/automation entitlement for the chosen login method. Integrate documented App Server, CLI headless, or API interfaces only when permitted. If account access is restricted, the correct state is `PROVIDER_BLOCKED`, not trying a different personal account.

## Threat-driven tests

Unauthorized chat query, duplicate callback, replay, forged event, stale approval, repo symlink escape, malicious test script, stdout flood, child process surviving timeout, daemon crash after external dispatch, provider quota error, and prompt-injected claim of owner approval.
