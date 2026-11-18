# Vision — Takt

## One sentence

A small Rust daemon that turns a high-level development request into **verified, coordinated work by multiple independent AI agents**, observable and controllable from terminal or phone.

## Why it exists

Existing terminal multiplexers and IDE shells can launch many coding agents, but launching sessions is not orchestration. The missing layer: break work into bounded tasks, validate dependencies, assign accountable owners, send messages, collect evidence, recover failures, and require explicit approval before integrating changes. The product should remain useful even with a **single Codex backend**.

## Target user

A solo developer who works in existing editors (such as Zed), wants several bounded agent workers on one machine, and wants to inspect progress or approve work remotely via Telegram. First operating system: Linux workstation. Initial use: personal local development. Future use: remote hosts, more adapters, optional Discord/MCP.

## Core experience

1. Register a local Git repository.
2. Submit a goal from CLI or authorized private Telegram chat.
3. Lead agent examines read-only repo context and proposes a **bounded DAG with acceptance criteria**.
4. Rust validates the DAG and the task scopes, then shows a plan (optionally gates large plans for approval).
5. Eligible tasks run concurrently in separate worktrees with explicit permissions.
6. Workers communicate via durable structured messages; the coordinator queues, routes, or escalates them.
7. Integration worktree receives reviewed commits; tests and independent review evaluate the combined result.
8. User sees changed files, commit IDs, check results, risks, and decides whether to merge.

## Non-goals (MVP)

- A general-purpose chatbot, shell replacement, or IDE.
- Fully autonomous unsupervised internet browsing or deployment.
- Consumer account credential pooling, quota circumvention, or unofficial OAuth relay.
- Training, hosting, or serving AI models.
- Distributed consensus / multiple competing daemon writers.
- Built-in generic browser automation, memory search, or skill marketplace.
- Perfect cross-model conversation history portability.

## Product promises

- **No fake completion**: task success requires checkable evidence and state transitions.
- **No invisible merge**: human-authorized merges; no automatic push/deploy.
- **Recoverable**: daemon restart preserves plan, audit log, and native session identifiers; uncertain executions are reconciled, not blindly rerun.
- **Inspectable**: one command shows who owns a task, its worktree, latest events, artifacts, and next required action.
- **Extensible but lean**: provider interface and event protocol are stable internal seams; features added only when a real end-to-end workflow demands them.

## Success criteria for first useful release

- One lead plans a workflow; two developers execute independent tasks concurrently; one reviewer evaluates the integrated output.
- Can safely pause, resume, and cancel without losing authoritative task state.
- Rejects cycles, missing dependencies, out-of-scope paths, and duplicate dispatch.
- A synthetic crash mid-run is recovered as INTERRUPTED; uncertain side effects aren't repeated automatically.
- Unauthorized Telegram accounts cannot query or mutate workflow state.
- No command uses blanket permission bypass by default.

## Positioning and naming

Working name Takt, repository `takt-rs`. **There are already actively developed AI projects called TAKT/takt**; validate naming and trademarks before publishing. Our implementation must be original. Distinction is a native, local-first daemon with strict state and permission boundaries, not copying another project's code or layout.
