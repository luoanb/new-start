# Requirements / 需求文档: ratatui-tui-redesign

## Restated Understanding / 需求复述

- 我理解当前需求是：重新梳理 `agent-app` TUI 方案，明确使用 `ratatui` 作为全屏 TUI 技术栈，替换当前基于 `std::io::read_line` 的轻量交互式 shell 体验。
- 当前核心目标是：把 TUI 打造成可恢复会话、上下文可见、聊天流清晰、输入体验可靠、任务状态明确的终端聊天工作台。
- 当前边界是：本轮先产出 `sdd-lab` 需求文档与技术方案，不进入代码实现；方案应基于当前 Rust core / CLI / Tauri 多入口架构，TUI 不应复制业务逻辑。
- 暂不处理：真实代码改造、依赖安装、终端视觉细节最终调参、跨平台打包策略、凭据加密存储、远端 models API 自动同步。

## Scope / 范围

- In:
  - 技术栈确定为 `ratatui`，终端事件后端使用 `crossterm`。
  - TUI 采用全屏 layout，不再依赖普通 `println!` 刷屏作为主要交互方式。
  - 顶部区域展示当前项目、当前 provider/model、当前配置状态、最近会话或会话列表入口。
  - 主区域展示聊天流，并区分 user、assistant、tool、error、status 等消息层级。
  - 底部输入框支持多行编辑、历史输入、快捷键和大段粘贴。
  - 工具执行以可折叠状态块展示：`running`、`done`、`failed`，允许展开查看详情。
  - 错误展示结构化说明：发生了什么、可能原因、下一步可选操作。
  - 会话可恢复：启动后能看到历史会话，选择并继续已有会话。
  - provider/model/config/session 切换通过轻量命令或命令面板完成，例如 `/model`、`/provider`、`/sessions`、`/config`。
  - 长任务展示运行感：状态、耗时、是否可取消、失败原因。
  - 保持与现有 `Gateway` / storage / provider registry 的职责边界一致。
- Out:
  - 不在本轮方案中直接实现代码。
  - 不把 TUI 业务逻辑写成独立分支；业务能力仍应从 Rust core 暴露。
  - 不要求第一版支持 token streaming；若后续需要，应先回写需求和方案。
  - 不要求第一版实现鼠标完整交互；键盘优先。
  - 不在 TUI 层直接读写 session 文件或 config 文件。

## User Interaction / 用户交互

- 触发入口：用户执行 `pnpm --filter agent-app tui` 或等价的 `cargo run --manifest-path src-tauri/Cargo.toml --bin agent-app-tui`。
- 用户操作路径：
  - 启动后进入全屏 TUI。
  - 顶部看到当前项目、当前会话、provider/model、配置状态。
  - 主区看到当前会话聊天历史；若无当前会话，则展示最近会话列表或新建会话提示。
  - 底部输入自然语言或 `/` 命令。
  - 使用 `/sessions` 切换会话，使用 `/provider` / `/model` 切换模型，使用 `/config` 查看配置状态。
  - 普通输入发送为当前会话消息。
- 系统反馈：
  - 用户消息、助手消息、工具调用、错误、状态更新使用不同样式和区块。
  - 模型调用或工具执行时展示 `running` 状态、耗时和取消提示。
  - 成功后状态变为 `done`，失败后状态变为 `failed` 并显示可执行建议。
- 状态变化：
  - 当前会话、当前模型、输入历史、滚动位置、折叠状态由 TUI app state 管理。
  - 会话消息持久化仍由 core/storage 负责。
  - provider/model 默认值从 `.agent-app/config.json` 读取，TUI 内选择只影响当前交互状态，是否持久化需在技术方案中明确。
- 异常/边界交互：
  - provider/model 未选择时，普通输入不发送请求，而是展示修复建议。
  - 缺少 API key、模型不存在、provider 请求失败时，不退出 TUI。
  - 长任务执行中允许取消；如果底层能力暂不支持真实取消，TUI 必须明确显示“不可取消”或“仅停止等待 UI”。
  - 粘贴大段文本不应破坏布局或立即误触发多次发送。
- 不应发生的交互：
  - 错误直接 dump stack trace 或导致进程退出。
  - 工具执行日志连续刷屏挤走聊天上下文。
  - 启动后像新进程一样丢失已有会话。
  - TUI 为了显示会话而绕过 core 直接改 `.agent-app/sessions` 文件。

## Acceptance Criteria / 验收标准

- [ ] `technical-plan.md` 明确使用 `ratatui + crossterm` 的 TUI 技术路线。
- [ ] 方案明确 TUI layout：顶部上下文区、主聊天流、底部多行输入区、可选侧栏/弹层。
- [ ] 方案明确 app state 和 core state 的职责边界。
- [ ] 方案明确消息展示模型，至少覆盖 user、assistant、tool、error、status。
- [ ] 方案明确工具执行状态块的 `running` / `done` / `failed` 展示和折叠策略。
- [ ] 方案明确错误展示结构，包含发生了什么、可能原因、下一步操作。
- [ ] 方案明确会话恢复和会话切换路径。
- [ ] 方案明确 `/model`、`/provider`、`/sessions`、`/config` 等轻量命令策略。
- [ ] 方案明确长任务进度、耗时、取消能力的最小实现边界。
- [ ] 方案列出执行步骤、风险、验证方式和进入执行前检查点。

## Constraints / 约束

- 业务约束：
  - 文档和代码冲突时，以文档为真相源，先同步文档再同步代码。
  - 没有用户明确确认技术方案前，不进入代码开发。
  - API key 和真实私密配置不得写入文档示例或提交。
- 技术约束：
  - TUI 入口继续是 Rust binary `agent-app-tui`。
  - TUI 不拥有业务状态持久化，必须通过 `Gateway` / core API 访问会话、配置、provider/model。
  - 新增 UI 状态应与 core domain state 分离，避免污染 storage 格式。
  - 依赖优先采用 Rust 社区成熟库：`ratatui`、`crossterm`、`ratatui-textarea`。
  - 如需历史输入和命令补全，优先在 Ratatui app 内实现轻量历史；不同时引入 full-screen TUI 与 readline 主循环。
- 时间/兼容性约束：
  - 第一版以 Linux/macOS/Windows 现代终端为目标，按 `crossterm` 能力设计。
  - 第一版键盘优先，鼠标交互可作为增强项。
  - 第一版不强制 token streaming；长任务状态与工具状态优先。

## Open Questions / 开放问题

- [ ] Q1 第一版是否需要 token streaming？
  - 当前建议：不纳入第一版，先做任务状态、耗时、错误和最终回复展示；streaming 另开迭代。
- [ ] Q2 `/model` 和 `/provider` 是否替代现有 `/use <provider> <model>`，还是保留 `/use` 作为兼容命令？
  - 当前建议：保留 `/use`，新增 `/model` 和 `/provider` 作为更轻量的交互入口。

## Requirement Decisions / 需求决策

- 2026-07-25 19:22:
  - 决策：TUI 方案技术栈使用 `ratatui`。
  - 原因：当前 `std::io::read_line` 方案无法支撑全屏布局、多区域状态、折叠工具块、多行输入和长期任务反馈。
- 2026-07-25 19:22:
  - 决策：本轮只产出 `sdd-lab` 需求和技术方案，不进入代码实现。
  - 原因：遵守 `No Spec, No Code` 与 `No Plan Approved, No Execute`。
