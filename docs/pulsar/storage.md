# Pulsar Storage

## Storage Root

The default storage root is:

```text
./.pulsar
```

The path is relative to the current working directory so development runs remain inspectable. A later release may move the default to an OS-specific app data directory, but that must be specified before changing behavior.

## Layout

```text
.pulsar/
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

Model provider configuration may be read from environment variables and `.pulsar/config.json`.

Environment variables have priority over file values for secrets and API bases. API keys must not be committed.
Model lists and chat defaults are configured in `.pulsar/config.json` so provider model name changes do not require Rust code changes.

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
  "poller": {
    "enabled": false,
    "base_interval_ms": 1000,
    "assistant_interval_ticks": 30
  },
  "neurons": {
    "bootstrap": {
      "create_neuron_prompt": "You are the Neuron Creator for an agent app. ... Return ONLY one JSON object ...",
      "system_prompts": {
        "assistant_select_neuron": "custom selector prompt (optional override)"
      }
    }
  }
}
```

Configuration fields:

- `defaults.provider`: provider selected by TUI session chat on startup.
- `defaults.model`: model selected by TUI session chat on startup.
- `poller.enabled`: whether automatic polling starts as Running (`true`, default since 2026-08-28; `false` to keep Paused). Manual step / `poll_trigger` still work when paused; runtime pause/resume APIs can override until next restart.
- `poller.base_interval_ms`: scheduler tick interval in milliseconds (default `1000`, minimum `1`).
- `poller.assistant_interval_ticks`: Assistant `PollAll` every N ticks (default `30`, minimum `1`). Effective auto-advance period ≈ `base_interval_ms × assistant_interval_ticks`.
- `providers.<id>.api_key`: provider API key. Environment variables override this value.
- `providers.<id>.api_base`: provider API base. Environment variables override this value.
- `providers.<id>.models`: provider model list shown by `/models` and accepted by `/use`.
- `neurons.bootstrap.create_neuron_prompt`: content of the unique `system_type=create_neuron` system neuron. Optional; when missing, the app uses a built-in default seed prompt so bootstrap can still create the first system neuron. The built-in seed asks for single-responsibility neurons with executable `content` (role / when-to-use / steps / output / constraints). Model-returned `weight` is ignored: new neurons and edges always start at weight `0` and only change via later evaluation deltas. Changing this config does not rewrite an already-persisted `create_neuron` row — reset/recreate that system neuron to pick up a new seed.
- `neurons.bootstrap.system_prompts.<system_type>`: optional per-type override of the built-in system prompt seeds (keys like `assistant_select_neuron` / `assistant_match_topic` / `assistant_complete_scope` / `assistant_score_feedback` / `assistant_revise_topic`). Non-empty values override the built-in seed; missing/empty falls back to the built-in seed. Built-in seeds let `ensure_system_neuron` / `rebootstrap` persist a known-good prompt without a model call; `system_type` values without a built-in seed still fall back to LLM generation.
- Assistant mode fixed `system_type` prompt neurons are ensured via `NeuronManager::ensure_system_neuron` / `bootstrap` (startup creates at least `assistant_select_neuron`). All built-in types use the built-in prompt seeds (see `SYSTEM_PROMPT_SEEDS` in `neuron/config.rs`) instead of LLM generation:
  - `assistant_select_neuron`: 7-candidate neuron selection
  - `assistant_match_topic`: topic match / create decision (lazy ensure)
  - `assistant_complete_scope`: afterhook scope completion decision (lazy ensure)
  - `assistant_score_feedback`: user satisfaction score (lazy ensure)
  - `assistant_revise_topic`: topic scope revision decision (lazy ensure)
- `bootstrap` also seeds one built-in **regular** neuron (not a system node): `通用助手` with initial weight `50` (see `BUILTIN_GENERIC_NEURON_SEED` in `neuron/config.rs`). It provides an out-of-the-box high-score default role — stable pick when LLM selection is unavailable (weight fallback) and a fallback candidate for normal semantic selection. Idempotent by exact `desc` match; existing nodes are never overwritten.
- Candidate pool rule: with `source_id`, only direct downstream; without source, global neurons including system nodes.

Missing optional neuron bootstrap configuration does not prevent application startup. Built-in default seed is used until overridden in config.

初始化流程图与阶段说明见 [neuron-init.md](./neuron-init.md)。

Known provider environment variables:

- `OPENAI_API_KEY`, `OPENAI_BASE_URL`
- `DEEPSEEK_API_KEY`, `DEEPSEEK_BASE_URL`
- `OLLAMA_API_KEY`, `OLLAMA_BASE_URL`
- `CUSTOM_OPENAI_API_KEY`, `CUSTOM_OPENAI_BASE_URL`
