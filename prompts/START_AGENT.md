# Copy/Paste Prompt — Start Development on Takt

You are the implementation agent for a new, independent Rust project named **Takt** (repository `takt-rs`). It must remain separate from my existing Orbit project and must not copy source code from projects with similar names.

Read **AGENTS.md**, **README.md**, **docs/VISION.md**, **docs/PRD.md**, **docs/ARCHITECTURE.md**, **docs/DOMAIN.md**, **docs/PROVIDERS.md**, **docs/SECURITY.md**, **docs/IMPLEMENTATION.md**, and **docs/TEST_PLAN.md** before coding. These documents are the design contract; if you discover inconsistencies, report a small proposed correction before major changes.

Goal: build a **hybrid multi-agent orchestrator**, not just a PTY multiplexer or CLI launcher. An on-demand lead agent plans; Rust validates a DAG, schedules bounded tasks, manages worker sessions and collaboration events, collects independent verification evidence, and requires human approval for merges. CLI/TUI/Telegram are frontends to one daemon.

**Implement ONLY Milestone M0 from prompts/M0_FOUNDATION.md in this run.** Build and test the deterministic core with a mock provider; do not add real Codex/Gemini integration, SQLite daemon, Telegram network calls, or account credentials yet. Preserve current repository changes and show your baseline commit/status.

Engineering policy:
- Official, permitted provider protocols only; no unofficial OAuth/token extraction, no quota/account rotation, no default YOLO bypass.
- No destructive Git operations, push, or deployment without my approval.
- Keep dependencies minimal; favor pure Rust logic for DAG and policy tests.
- Include observable acceptance tests and honest evidence. Run cargo fmt/clippy/test if available.
- Report architecture deviations, risks, and next milestone after M0.

Return: (1) brief implementation plan, (2) actual changes, (3) tests with results, (4) remaining gaps, and (5) recommendation for M1. Don't claim future features are implemented.
