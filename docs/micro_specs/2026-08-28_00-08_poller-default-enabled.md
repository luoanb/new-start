# Micro Spec: Poller 默认开启自动轮询

- 日期：2026-08-28 00:08
- 状态：待批准执行
- 取代：`2026-08-01_05-28_poller-default-paused.md`（决策翻转）

## 1. 背景（Reverse Sync：决策翻转）

2026-08-01 决策「默认不自动轮询」（Poller 启动即 `Paused`，日常手动步进；
`config.json → poller.enabled: true` 才自动推进）。本次按用户要求翻转：

**config.json 缺省 `poller` 段（或 `enabled` 字段）时，Poller 默认开启自动轮询。**

## 2. 改动

| 位置 | 现状 | 改为 |
| --- | --- | --- |
| `poller.rs` `PollerSettings::default()` | `enabled: false` | `enabled: true` |
| `poller.rs` `PollerConfigReader::load()` | `section.enabled.unwrap_or(false)` | `section.enabled.unwrap_or(true)` |

语义：显式写 `"poller": {"enabled": false}` 仍关闭；缺失字段回落到开启。

## 3. 文档同步（Reverse Sync）

- `docs/pulsar/storage.md` L115：`enabled` 默认描述 `false` → `true`。
- `docs/sdd-lab/2026-07-26_21-30_assistant-mode/requirements.md` L19：
  「默认不自动轮询」→「默认自动轮询」。
- `2026-08-01_10-45_poller-config.md` L26：enabled 默认 `false` → `true`（标注被本 spec 更新）。
- `2026-08-01_05-28_poller-default-paused.md`：标注被本 spec 取代。

## 4. 风险与影响

- **现有用户 config.json 无 `poller` 段 → 升级后自动轮询开启**，会自动推进课题并
  调用模型 API（消耗 token）。用户可显式 `enabled: false` 关闭。
- 运行时可随时 `poll_pause` / `poll_resume` 覆盖，重启后回落配置值。

## 5. 验证

- 新增单测：config.json 缺 `poller` 段 → `load()` 返回 `enabled == true`；
  显式 `enabled: false` → 仍为 false。
- `cargo test --lib` 全量通过。
