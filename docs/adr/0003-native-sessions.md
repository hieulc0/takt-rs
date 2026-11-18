# ADR-0003: Native provider session IDs; no universal PTY session guarantee

- **Status:** accepted for MVP
- **Decision:** use official provider conversation/turn APIs when available. Persist native session references and capability flags; one active turn per session. Generic PTYs only for later adapters that require a terminal. `resume` is adapter-specific and may be unsupported.
- **Consequences:** structured events and approvals are easier to supervise. Reboot recovery cannot promise that any ongoing external tool command completed; mark uncertain attempts interrupted.
- **Rejected:** screen-scraping terminal output as the primary protocol, shared login cache as orchestrator state, pretending context handoff is native session resume.
