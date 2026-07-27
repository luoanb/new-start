# Lifecycle / 生命周期: Assistant 运行模式

```yaml
status: executing
result: pending
created_at: 2026-07-26 21:30
updated_at: 2026-07-28 01:03
owner: user
```

## Current Summary / 当前摘要

- 批准状态：修正版技术方案已获用户确认，本轮代码重构已执行
- 当前状态：executing
- 当前核心目标：按通用 Hook 协议重构 Assistant 业务 Hook 代码路径
- 下一步唯一动作：审阅本轮代码变更，确认是否继续补齐候选不足时自动创建神经元与会话回滚能力

## Execution Log / 执行记录

- 1. 2026-07-26 21:30: 创建迭代，需求复述已获用户确认。
- 2. 2026-07-26 21:30: 需求文档更新：新增 AfterHook、关闭开放问题（按默认值处理）。
- 3. 2026-07-26 21:30: 需求文档更新：修正流水线顺序、新增 PreHook0、课题绑定规则。
- 4. 2026-07-26 21:30: 技术方案更新：NeuronSelector 撤销，逻辑并入 NeuronStore。
- 5. 2026-07-26 21:30: 技术方案更新：Poller 改为通用调度器（多任务、倍数间隔），业务方自定间隔。
- 6. 2026-07-27 00:00: 用户确认技术方案，进入 execute 阶段。开始实施 Step 1～6。
- 7. 2026-07-28 00:57: Reverse Sync：执行后发现 Hook 分层不符合预期。需求修正为 PreHook1 负责神经元上下文准备；`docs/design/hook-spec.md` 升级为对话模式通用 Hook 协议；技术方案回退到 planned，等待用户确认后再重构代码。
- 8. 2026-07-28 01:03: 用户确认按新方案更新代码。新增 `core/hooks` 通用协议类型与 Assistant 业务 Hook；`Gateway::run_assistant_round` 改为 PreHook2 → PreHook1 → Assistant Engine → AfterHook 编排；验证通过 `cargo check` 与 `cargo test`。
