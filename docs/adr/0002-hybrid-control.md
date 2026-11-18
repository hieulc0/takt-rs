# ADR-0002: Hybrid LLM planning with deterministic control

- **Status:** accepted for MVP
- **Context:** free-form agent-to-agent chat lacks testable execution boundaries.
- **Decision:** lead proposes task DAG and resolves blockers; Rust validates, schedules, assigns, routes mail, records events, verifies checks and enforces human approval. Every task is bounded by paths, acceptance and retry budget.
- **Consequences:** predictable recovery and resource ceilings; lead can still be creative about plan and task decomposition. Slightly more upfront schema work; worth it for reliable execution.
- **Rejected:** an always-running central LLM process as sole source of truth, unrestricted peer chat loops, provider-specific orchestration embedded in adapters.
