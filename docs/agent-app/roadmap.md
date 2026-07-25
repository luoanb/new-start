# Agent App Roadmap

## Milestone 1: Local Rust Core

Acceptance criteria:

- Rust core defines shared domain models and error types.
- Conversations can be created, listed, cleared, and inspected.
- Sending a message persists user and assistant messages.
- Built-in skills can be listed.

## Milestone 2: Tauri Thin Adapter

Acceptance criteria:

- Tauri command handlers call Rust core instead of owning business logic.
- Frontend can send a message, see the response, list skills, and see runtime status.
- Template-only `greet` behavior is removed.
- Cargo default run target remains the GUI binary so `pnpm tauri dev` can start without `--bin`.

## Milestone 3: CLI Entry

Acceptance criteria:

- CLI binary exposes `chat`, `skills`, `sessions`, `history`, `clear`, and `status`.
- CLI exits with documented status codes.
- CLI uses the same core storage and conversation behavior as Tauri.

## Milestone 4: TUI Entry

Acceptance criteria:

- TUI binary starts an interactive terminal session.
- Slash commands map to the shared command model.
- Non-command input sends chat messages through core.

## Milestone 5: Verification

Acceptance criteria:

- Core unit tests cover conversation lifecycle, skill listing, and validation errors.
- Rust formatting and tests pass.
- Svelte/Tauri frontend type checks pass.

## Later Milestones

- LLM provider integration.
- Config persistence.
- Streaming responses.
- Full-screen TUI layout.
- Installer-level CLI/TUI bundling.
