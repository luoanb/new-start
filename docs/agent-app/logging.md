# Agent App Runtime Logging

## Purpose

Unified runtime logs for diagnosing bootstrap and other core flows. Logs go to:

1. Rolling files under `{storage_root}/logs/`
2. In-memory ring buffer (2000 entries)
3. GUI bottom **Logs** panel via Tauri event `app://logs`

## Defaults

| Item | Value |
| --- | --- |
| Default level | `info` |
| Override env | `RUST_LOG` or `AGENT_APP_LOG` (`error\|warn\|info\|debug\|trace`) |
| File | `agent-app.log`, rotate at **8MB**, keep **5** archives (`agent-app.log.1` …) |
| GUI buffer | last 2000 entries; panel filter view last 500 matches |

## GUI

Bottom Panel → **Logs**:

- **Verbosity**: changes backend filter (`logs_set_level`)
- **Min level / Target / Keyword**: client-side filters
- **Clear**: clears ring buffer only (files kept)
- Path hint shows log directory

## Neuron bootstrap fields

Look for `phase=` among:

- `bootstrap_neurons` / `bootstrap` / `rebootstrap`
- `ensure_creator` / `ensure_system_neuron`
- `select_candidates` / `select_one`
- `generate_draft` / `fill_candidate_neuron`

Useful keywords: `assistant_select_neuron`, `generate_draft`, `error_code`.

## Assistant converse fields

User-input path (`ConversationMode::Assistant`):

- `send_model_message` → `assistant_converse`
- steps: `score_feedback` → `match_topic` → `select_neuron` → `run_core` → `complete_scope`
- nested: `assistant_system_json` (match/score/complete prompts), `match_topic_hook`, `select_neuron_hook`, `assistant_run_core`

If the UI hangs with no reply, filter keyword `assistant_converse` or `assistant_system_json` and see which `step=` last appeared without a following `ok` / `failed`.

On JSON/model failures, look for `user_preview` / `output_preview` (redacted + truncated) next to `error_code`.

## Entrypoints

- **GUI**: init + emit in Tauri `setup`; also runs `bootstrap_neurons`
- **CLI**: file + stderr
- **TUI**: file only (stderr fmt disabled so the terminal UI is not corrupted)
