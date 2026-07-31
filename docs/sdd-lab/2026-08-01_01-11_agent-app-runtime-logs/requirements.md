# Requirements / 需求文档: Agent App 运行日志

## Restated Understanding / 需求复述

- 我理解当前需求是：为诊断神经元 bootstrap 等问题，建立统一运行日志；日志既要落盘，也要在 GUI 底部已有 **Logs** 面板中可见；要有足够过滤能力便于快速定位；并处理日志文件过大问题。
- 当前核心目标是：关键路径（尤其神经元初始化）可追踪，且前台能实时/准实时查看与过滤。
- 当前边界是：agent-app Rust core + Tauri GUI Logs 面板；TUI/CLI 至少受益于文件日志（TUI 不强制内嵌日志 UI）。
- 暂不处理：远程日志采集、完整 prompt/密钥落盘、分布式 tracing。

## Scope / 范围

### In

- 统一日志设施（级别、target/模块、时间戳、结构化字段）。
- **落盘**：写入应用存储目录下的日志文件（建议 `.agent-app/logs/`）。
- **前台可见**：填充现有底部 Panel 的 Logs 占位（`panelViews.logs`），展示近期日志流。
- **过滤**：至少支持按级别、模块/target、关键字；可选时间范围。
- **体积治理**：滚动/轮转 + 数量或总大小上限，避免无限增长。
- 神经元初始化路径优先打点（bootstrap / ensure / select / generate_draft 等）。

### Out

- 不把完整 system/user prompt 或 API key 写入默认日志。
- 不做服务端日志聚合。
- 本期不重做整站可观测平台。

## User Interaction / 用户交互

- 触发入口：底部 Panel → **Logs** tab（容器已存在，现为 placeholder）。
- 用户操作路径：
  1. 打开 Logs 面板，看到近期日志（新到旧或旧到新，需一致并标明）。
  2. 用过滤器缩小范围（级别 / 模块 / 关键字）。
  3. 可选：清空面板缓冲、打开/揭示日志目录（若易实现）。
- 系统反馈：新日志准实时追加；过滤结果即时更新；无匹配时有空态。
- 异常：后端日志子系统失败不得拖垮主流程；面板显示降级提示即可。

## Filtering / 过滤选项（最低集）

| 过滤器 | 说明 |
| --- | --- |
| level | `error` / `warn` / `info` / `debug`（及“不低于某级”） |
| target / module | 如 `neuron`、`gateway`、`assistant`、`provider` |
| keyword | 匹配 message 或关键字段（id、system_type、error code） |
| time（可选本期） | 最近 N 分钟 / 自定义范围 |

日志条目建议字段：`ts`、`level`、`target`、`message`、可选 `fields`（如 `system_type`、`phase`、`error_code`）。

## File Size / 文件体积

- 使用滚动策略，推荐二选一（技术方案定一种）：
  - **按大小**：单文件上限（如 5–10MB）+ 保留最近 N 个（如 5）；或
  - **按日 + 总上限**：每日一个文件，最多保留 D 天。
- 超出后自动删除最旧文件；不阻塞业务。
- 配置可后续暴露；本期可用合理默认值写死或放 `config.json` 可选段。

## Frontend Buffer / 前台缓冲

- GUI 展示的是**内存环形缓冲**（或等价有界队列），不是整文件全文加载。
- 缓冲条数上限（如 1000–5000），超出丢弃最旧。
- 文件仍完整保留滚动历史；面板只看“最近一段”。

## Acceptance Criteria / 验收标准

- [x] 启动或 bootstrap 时，关键阶段写入文件日志且带 level/target。
- [x] GUI Logs 面板不再是 placeholder，能显示日志条目。
- [x] 支持 level + target/module + keyword 过滤，并能快速定位 neuron bootstrap 失败点。
- [x] 日志文件有轮转/上限，不会无限增大。
- [x] 默认不记录密钥与完整 prompt。
- [x] 日志子系统故障不导致 Gateway 启动失败。

## Constraints / 约束

- 业务约束：诊断优先；默认 info，可用环境变量或配置提高详细度。
- 技术约束：复用底部 Logs 容器；Tauri 通过事件推送或轮询拉取有界日志。
- 兼容：TUI 运行时避免刷屏破坏界面——前台以 GUI 为主；TUI/CLI 依赖文件日志（+ 启动阶段 stderr 可选）。

## Open Questions / 开放问题

- [x] Q1 滚动策略偏好：按大小（5MB×5）还是按天（保留 7 天）？
  - 决策：A — 按大小（技术方案默认单文件 8MB × 保留 5 个）。
- [x] Q2 GUI 日志刷新：Tauri `emit` 实时推送，还是面板打开时轮询？
  - 决策：A — 实时 emit；面板打开时再用 snapshot 补历史。
- [x] Q3 默认级别：`info` 是否合适？是否需要 GUI 内切换 verbosity？
  - 决策：默认 `info`；GUI 内可切换 verbosity。

## Requirement Decisions / 需求决策

- 2026-08-01 01:11:
  - 决策：Logs 必须前台可见（填现有容器）；必须有过滤；必须考虑文件过大。
  - 原因：用户明确补充三点，用于分析初始化未完成等问题。
- 2026-08-01 01:14:
  - 决策：关闭 Q1=A、Q2=A、Q3=是（默认 info + GUI 可调）。
  - 原因：用户逐条确认。
