# Agent App Errors

## Error Shape

Core errors have a stable code and user-facing message. Entry layers may add presentation details but must not rewrite the meaning.

## Codes

- `invalid_input`: user input is empty or malformed.
- `conversation_not_found`: a requested conversation id does not exist.
- `skill_not_found`: a requested skill name does not exist.
- `provider_not_found`: a requested provider id does not exist.
- `model_not_found`: a requested model id does not exist for the provider.
- `provider_auth_missing`: provider credentials were not found in env or config.
- `llm_request_failed`: the provider request failed.
- `storage_error`: reading, writing, or deleting persisted data failed.
- `runtime_error`: the agent runtime failed after input validation succeeded.

## Presentation Rules

- CLI prints user-facing errors to stderr.
- TUI renders user-facing errors in the session output.
- Tauri returns serializable error payloads to the frontend.
- Debug details stay in logs or developer output and are not required for the first milestone.

## Exit Codes

- `0`: success.
- `1`: runtime or storage failure.
- `2`: invalid input or missing user-controlled resource.
