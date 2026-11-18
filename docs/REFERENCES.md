# External References — checked November 18, 2026

Reference documentation informs adapter design, but **the installed tool version is the implementation authority for supported message schemas**. Recheck before coding.

- Codex App Server (local stdio, thread and turn lifecycle, approval requests, schema generation): https://learn.chatgpt.com/docs/app-server
- Codex CLI developer commands (`codex exec`, JSONL and resume): https://learn.chatgpt.com/docs/developer-commands
- Codex CLI reference: https://learn.chatgpt.com/docs/cli/reference
- Gemini CLI headless structured output: https://geminicli.com/docs/cli/headless/
- Gemini CLI native session management: https://geminicli.com/docs/cli/session-management/
- Teloxide framework (polling, webhooks): https://github.com/teloxide/teloxide
- Teloxide Rust API: https://docs.rs/teloxide/latest/teloxide/

## Existing naming conflicts

- https://github.com/nrslib/takt — existing AI orchestration tool named TAKT.
- https://github.com/oscarrenalias/takt — another agent runner/orchestrator exposing the `takt` command.

Repository name `takt-rs` is a temporary distinguishing suffix, **not a trademark or global CLI uniqueness guarantee**. Consider a new public name prior to release; do not copy the code, tests, or internal workflows of similarly named projects.

## Claims not made

- No claim that all subscription credentials may be used for unattended automation or in third-party apps.
- No claim that Git worktrees confine agents to a directory.
- No claim that native provider history is portable across providers.
- No claim that this starter has implemented its planned daemon, App Server, or Telegram integration.
