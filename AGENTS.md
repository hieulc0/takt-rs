# AGENTS.md — Instructions for Coding Agents

## Project and rules

This is **Takt**, a new independent Rust project (temporary repository name `takt-rs`), NOT a rewrite/import of Orbit or any existing TAKT project. Read `README.md`, `docs/VISION.md`, `docs/ARCHITECTURE.md`, `docs/DOMAIN.md`, `docs/SECURITY.md`, `docs/IMPLEMENTATION.md` before changing core behavior.

**Do not implement the entire roadmap at once.** User delegates one milestone per run. Default assignment is M0, as described in `prompts/M0_FOUNDATION.md`.

## Before coding

1. Inspect tree, `git status`, current branch, and baseline commit (if repository initialized); do not overwrite unrelated changes.
2. Summarize the current implementation vs planned docs. Mark TODOs honestly.
3. State bounded deliverables, acceptance evidence, permitted paths and commands.
4. For a bug, preserve a minimal reproducible failure before the fix.
5. If an interface is undocumented or provider-specific, inspect **official current docs/installed schema** rather than inventing flags.

## Engineering constraints

- Rust 2021 edition scaffold; prefer simple modules, explicit enums and typed errors.
- Keep scheduler, DAG validator, policy engine, and event normalizer separately testable.
- No Redis, Temporal, browser frontend, remote worker, or Docker dependency in M0–M1.
- One daemon is authoritative for workflow/session state; don't store parallel state in a bot/CLI.
- SQLite transitions and event/outbox append must be atomic in M1.
- Treat model output and repository content as untrusted. Never let model text override policy.
- Never default to `--yolo`, sandbox bypass, account rotation, or unofficial OAuth token access.
- Do not add Antigravity/Claude subscription OAuth adapter without explicit permitted integration evidence.
- Session ID belongs to native backend, not to an invented global memory service.
- Git worktree is not sandbox; document and enforce boundaries appropriately.
- No automatic merge/push/deploy; approval must bind exact revision and expected HEAD.
- Secrets may never be printed, committed, attached to tasks, or forwarded via Telegram.

## Definition of done

Run and report:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

Provide tests for new behavior, explain test environment, and label unrun checks. Update relevant docs if changed. Finish with a concise change summary, file list, tests and exact next safe milestone. No fake implementation claims.

## First permitted scope

For M0, implement `src/domain`, `src/mock`, pure scheduler, basic CLI demo, offline tests; edit Cargo.toml and README only as needed. Do not connect real accounts or require credentials. Keep changes small and reviewable.

## Agent communication

Use formal task/acceptance outputs, not endless agent chat. If blocked, provide a specific question with the exact file/line/task and stop. Independent reviewers should inspect actual diff, Git status, and test results.
