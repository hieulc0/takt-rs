# Local Operations (planned commands)

## Environments

Linux first. Use XDG directories:
- Config: `${XDG_CONFIG_HOME:-$HOME/.config}/takt-rs/config.toml`
- Data: `${XDG_DATA_HOME:-$HOME/.local/share}/takt-rs/state.sqlite3`
- Runtime: `${XDG_RUNTIME_DIR}/takt-rs/daemon.sock`
- Cache/log: `${XDG_CACHE_HOME:-$HOME/.cache}/takt-rs/`

Directories storing credentials are `0700`, token files `0600`, socket `0600`. On missing `XDG_RUNTIME_DIR`, choose a private fallback with ownership validation; never share `/tmp/takt.sock` openly.

## Getting started with the scaffold (available now)

```bash
cargo test
cargo run -- --help
cargo run -- doctor
```

A Rust toolchain is required; if offline, build after installing it. No dependencies in initial Cargo.toml. No background daemon yet.

## Future installed usage (not implemented)

```bash
takt daemon start
takt project add steadlume ~/projects/steadlume
takt run steadlume 'Improve login flow with tests'
takt workflow status <id>
takt events tail <id>
takt tui
```

## Telegram setup (future)

1. Create a bot using official BotFather UI and keep the token private.
2. Set token through protected file/environment; do not put it in `config.toml` or git.
3. Identify exact numeric owner user ID and private chat ID; set allowlists.
4. Run `takt telegram doctor` locally; verify unauthorized access is refused.
5. Enable long polling in daemon; no public endpoint required.

## Recovery operations

- Graceful SIGTERM: stop claiming new tasks; request cancellation or checkpoint; flush events; exit.
- Ungraceful crash: reconnect DB; verify schema; mark active attempts RECONCILING/INTERRUPTED; inspect native session state. Do not silently rerun unknown external operations.
- Restore from database backup only with daemon stopped or SQLite online backup API; WAL file consistency matters.
- Keep an append-only audit trace of merge approvals and outcomes. Reconciliation report is accessible from CLI.
- Worktree cleanup only for terminal tasks and clean/verified branches; never use destructive reset on user's working tree.

## Logs and privacy

Structured logs use correlation IDs, level and component. Bound log size and retention, redact auth tokens/provider payloads, and avoid sending full source to Telegram. Debug protocol dumps require explicit opt-in and should not contain secrets.

## systemd user service (future)

Ship a template after M1 with `ExecStart=takt daemon serve`, `Restart=on-failure`, sane limits. A running user service should own the socket; don't launch duplicates from TUI. For Telegram when workstation sleeps, polling stops; the design does not claim 24/7 availability.
