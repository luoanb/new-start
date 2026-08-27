# Spec: Poller 默认关闭自动轮询

> **已废弃（2026-08-28）**：决策翻转，Poller 缺省改为自动轮询开启。
> 取代者：`docs/micro_specs/2026-08-28_00-08_poller-default-enabled.md`。

## Goal

- 默认不自动轮询；用户手动步进（`assistant step` / `poll_trigger`）。
- 仍可 `poll_resume` 打开自动轮询。

## Done Contract

- `Poller::new` 初始状态为 `Paused`；Gateway 启动后默认 paused（可用 `config.json` → `poller.enabled` 覆盖，见 `2026-08-01_10-45_poller-config.md`）。
- pause 下 `trigger` 仍可在下一 tick 强制跑一轮；`assistant_step` 不受影响。
- `cargo test --lib` 通过。

## Change

- `poller.rs`：`new()` → `Paused`；单测先 `resume()` 再测间隔触发。
- 反写 assistant-mode 需求一句：默认不自动轮询。

## Validation

- `cargo test --lib poller`：4 passed（含 default paused、trigger 在 pause 下仍可用）。
