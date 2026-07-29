# Agent App Storage

## Storage Root

The default storage root is:

```text
./.agent-app
```

The path is relative to the current working directory so development runs remain inspectable. A later release may move the default to an OS-specific app data directory, but that must be specified before changing behavior.

## Layout

```text
.agent-app/
  config.json
  sessions/
    <conversation-id>.json
```

## Conversation Format

Each conversation file stores:

- `id`: stable conversation id.
- `messages`: ordered messages.
- `created_at`: Unix timestamp in milliseconds.
- `updated_at`: Unix timestamp in milliseconds.

Each message stores:

- `role`: `user`, `assistant`, or `system`.
- `content`: message body.
- `timestamp`: Unix timestamp in milliseconds.

## Consistency Rules

- Core is the only layer allowed to read or write conversation files.
- Entry layers pass conversation ids and user input into core; they do not mutate storage.
- Missing conversations are created on first write.
- Clearing a conversation removes its persisted file when persistence is enabled.
- Corrupt session files are ignored by listing APIs and surfaced as storage errors only when directly requested.
- Generated conversation ids must remain unique across rapid create/clear/create cycles.

## Configuration

Model provider configuration may be read from environment variables and `.agent-app/config.json`.

Environment variables have priority over file values for secrets and API bases. API keys must not be committed.
Model lists and chat defaults are configured in `.agent-app/config.json` so provider model name changes do not require Rust code changes.

```json
{
  "defaults": {
    "provider": "deepseek",
    "model": "deepseek-v4-flash"
  },
  "providers": {
    "openai": {
      "api_key": "sk-...",
      "api_base": "https://api.openai.com/v1",
      "models": [
        {
          "id": "gpt-4o-mini",
          "display_name": "GPT-4o mini",
          "capabilities": {
            "chat": true,
            "tools": true,
            "streaming": true
          }
        }
      ]
    },
    "deepseek": {
      "api_key": "...",
      "api_base": "https://api.deepseek.com/v1",
      "models": [
        {
          "id": "deepseek-v4-flash",
          "display_name": "DeepSeek V4 Flash",
          "capabilities": {
            "chat": true,
            "tools": false,
            "streaming": false
          }
        }
      ]
    },
    "custom": {
      "api_key": "...",
      "api_base": "https://example.com/v1"
    }
  },
  "neurons": {
    "bootstrap": {
      "create_neuron_prompt": "Create one neuron and return only JSON with desc, content, weight, and tool_ids."
    }
  }
}
```

Configuration fields:

- `defaults.provider`: provider selected by TUI session chat on startup.
- `defaults.model`: model selected by TUI session chat on startup.
- `providers.<id>.api_key`: provider API key. Environment variables override this value.
- `providers.<id>.api_base`: provider API base. Environment variables override this value.
- `providers.<id>.models`: provider model list shown by `/models` and accepted by `/use`.
- `neurons.bootstrap.create_neuron_prompt`: content of the unique `system_type=create_neuron` system neuron. Optional; when missing, the app uses a built-in default seed prompt so bootstrap can still create the first system neuron.
- Assistant mode fixed `system_type` prompt neurons are ensured via `NeuronManager::ensure_system_neuron` / `bootstrap_ready` (startup creates at least `assistant_select_neuron`):
  - `assistant_select_neuron`: 7-candidate neuron selection
  - `assistant_match_topic`: topic match / create decision (lazy ensure)
  - `assistant_complete_scope`: afterhook scope completion decision (lazy ensure)
  - `assistant_score_feedback`: user satisfaction score (lazy ensure)
- Candidate pool rule: with `source_id`, only direct downstream; without source, global neurons including system nodes.

Missing optional neuron bootstrap configuration does not prevent application startup. Built-in default seed is used until overridden in config.

Known provider environment variables:

- `OPENAI_API_KEY`, `OPENAI_BASE_URL`
- `DEEPSEEK_API_KEY`, `DEEPSEEK_BASE_URL`
- `OLLAMA_API_KEY`, `OLLAMA_BASE_URL`
- `CUSTOM_OPENAI_API_KEY`, `CUSTOM_OPENAI_BASE_URL`
