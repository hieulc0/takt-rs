# ADR-0004: Telegram is a remote control, not a shell

- **Status:** accepted for MVP
- **Decision:** long polling through Teloxide; numeric private user/chat allowlist, bounded commands, redacted summaries, two-step one-time approval. Calls same command service and policy checks as CLI.
- **Consequences:** no public server requirement, minimal attack surface, operation stops when PC is off. Remotely submitted natural-language goals still require controlled execution policy.
- **Rejected:** Telegram command to execute arbitrary shell, username-based authorization, auto YOLO, direct DB access by bot.
