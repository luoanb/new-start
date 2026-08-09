# Pulsar Commands

## Command Model

Commands are shared conceptually by CLI, TUI, and GUI. CLI uses explicit subcommands, TUI exposes the same behavior through key bindings and command prompts, and GUI exposes the same behavior through controls.

## Required Commands

### `chat`

Send a user message to the current or selected conversation.

- Input: message text, optional conversation id, selected provider/model.
- Output: assistant response and conversation id.
- Core API: `Gateway::send_model_message`.
- TUI requires an active provider/model before ordinary input can be treated as chat.

### `skills`

List built-in skills.

- Input: none.
- Output: skill name and description.
- Core API: `Gateway::list_skills`.

### `sessions`

List persisted conversations.

- Input: none.
- Output: id, message count, created timestamp, updated timestamp.
- Core API: `Gateway::list_conversations`.

### `history`

Show messages in a conversation.

- Input: optional conversation id.
- Output: ordered messages with role, content, and timestamp.
- Core API: `Gateway::history`.

### `clear`

Clear a conversation.

- Input: optional conversation id.
- Output: cleared conversation id.
- Core API: `Gateway::clear_conversation`.

### `status`

Show runtime status.

- Input: none.
- Output: app name, storage path, current conversation id, skill count, conversation count.
- Core API: `Gateway::status`.

### `providers`

List configured model providers.

- Input: none.
- Output: provider id, display name, auth environment variable, optional API base.
- Core API: `Gateway::list_providers`.

### `models`

List available models.

- Input: optional provider id.
- Output: model id, provider id, display name, capabilities.
- Core API: `Gateway::list_models`.

### `use-model`

Select the provider/model used by session chat.

- Input: provider id and model id.
- Output: selected provider/model.
- Core API: `Gateway::require_model`.
- TUI command: `/use <provider> <model>`.
- This affects ordinary TUI input only. Stateless model calls still pass provider/model explicitly.

### `call-model`

Call a model without reading or writing local sessions.

- Input: provider id, model id, one or more messages.
- Output: provider id, model id, model output.
- Core API: `Gateway::call_model`.

## CLI Behavior

- Successful commands exit with code `0`.
- User or validation errors exit with code `2`.
- Storage/runtime errors exit with code `1`.
- Default output is human-readable text.
- `--json` is reserved for future structured output and should not be faked before the response schema is specified.
- `call-model --provider <id> --model <id> <message>` performs a stateless model call.
- CLI `chat` may require explicit provider/model flags or configured defaults once provider-backed session chat is enabled.

## TUI Behavior

The TUI is a compact interactive shell:

- Shows status on startup.
- Accepts `/help`, `/skills`, `/sessions`, `/history`, `/clear`, `/status`, and `/exit`.
- Accepts `/providers`, `/models [provider]`, `/use <provider> <model>`, and `/call <provider> <model> <message>`.
- Loads the active provider/model from configured defaults when possible.
- Treats any non-command input as session chat only after a provider/model is selected.
- Keeps `/call` stateless: it does not read or write conversation history.
- Prints recoverable command/provider/model errors and returns to the prompt instead of exiting.
- Reuses the same core gateway as CLI and Tauri.
- Displays the selected provider/model in the prompt.

Full-screen terminal layout can be added later once the command model is stable.
