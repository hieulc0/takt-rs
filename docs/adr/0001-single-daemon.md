# ADR-0001: Single local Rust daemon, SQLite, Unix socket

- **Status:** accepted for MVP
- **Context:** local-first, low resource use, single user, multiple agent sessions, no full IDE.
- **Decision:** one Rust daemon exclusively owns SQLite writes; CLI/TUI/Telegram call the same command service over local transport. Tokio event notifications, SQLite durable journal and outbox. Unix socket for local CLI; Telegram transport lives in daemon and calls service directly.
- **Consequences:** easy state consistency, no broker infrastructure, simple backup; not distributed or highly available. Recovery still needed for processes outside SQLite. Remote nodes are postponed.
- **Rejected:** Redis/Temporal for MVP, full web backend, ad hoc per-agent shell scripts with no authoritative task state.
