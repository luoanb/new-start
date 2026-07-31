# Requirements / 需求文档: agent-app-topic-poller-mgmt

## Restated Understanding / 需求复述

- 我理解当前需求是：在 agent-app GUI 中新增 Topic（课题）管理界面和 Poller（轮询调度器）管理界面。Topic 和 Poller 是 Assistant 模式的核心支撑组件 —— Topic 管理会话的目标和进度，Poller 驱动 Assistant 模式的时间调度。
- 当前核心目标是：将 TUI 中通过 `/topic` 和 `/poll` 命令管理的功能搬到 GUI，提供可视化的 Topic 列表/详情/编辑和 Poller 状态监控/控制界面。
- 当前边界是：Rust 后端 TopicStore/TopicManager 和 Poller 层已完整实现，需要新增 Tauri 命令暴露这些能力到前端。
- 暂不处理：Topic 的自动创建和绑定流程（由 Assistant 模式自行管理）、Topic 的统计分析图表。

## Scope / 范围

### In

1. **Rust 后端 — 新增 Tauri 命令**
   - `list_topics(status_filter?: string) -> Vec<Topic>`：列出课题，可选按状态过滤
   - `get_topic(id: string) -> Topic`：获取单个课题详情
   - `create_topic(name: string, description?: string) -> Topic`：创建新课题
   - `update_topic(id: string, name?: string, description?: string) -> Topic`：更新课题属性
   - `delete_topic(id: string) -> bool`：删除课题
   - `add_topic_scope_item(topic_id: string, goal: string, done_contract: string) -> Topic`：添加范围项
   - `delete_topic_scope_item(topic_id: string, item_id: string) -> Topic`：删除范围项
   - `complete_topic_scope_item(topic_id: string, item_id: string) -> Topic`：完成范围项
   - `pause_topic(id: string) -> Topic`：暂停课题
   - `resume_topic(id: string) -> Topic`：恢复课题
   - `poll_status -> PollerStatus`：获取 Poller 运行状态
   - `poll_pause -> ()`：暂停 Poller
   - `poll_resume -> ()`：恢复 Poller
   - `poll_trigger -> ()`：触发 Poller 立即执行所有 handler

2. **Topic 管理界面**
   - 课题列表页面/面板：展示所有课题（ID、名称、状态标签、进度百分比、更新时间）
   - 状态过滤：按全部/todo/in_progress/paused/done/cancelled 筛选
   - 创建新课题：表单（名称、描述），创建后自动跳转到详情
   - 课题详情页：完整展示课题信息（名称、描述、状态、进度、scope items、extra 数据）
   - Scope items 管理：在详情页内展示列表，支持添加/完成/删除 scope item
   - 课题状态操作：暂停/恢复课题
   - 删除课题：二次确认弹窗
   - 课题列表标记当前会话绑定的课题

3. **Poller 管理界面**
   - Poller 状态面板：展示当前状态（running/paused）、tick 计数、任务数、base interval
   - 控制按钮：Pause / Resume / Trigger
   - 状态变化实时反馈

### Out

- Topic 自动绑定会话流程（由 Assistant 模式后台管理）
- Topic 统计图表或数据分析视图
- Poller 日志历史或执行记录列表
- 多 Poller 实例管理
- Topic 批量操作（批量删除/暂停）

## User Interaction / 用户交互

- **触发入口**：侧栏面板中新增 "Topics" 和 "Poller" 标签页，或在导航中增加入口。
- **用户操作路径（Topic）**：
  1. 点击侧栏 "Topics" 标签 → 展示课题列表（含状态筛选项）
  2. 点击 "+" 或 "新建课题" 按钮 → 弹出新建表单（名称、描述）→ 提交后创建并进入详情
  3. 点击课题列表项 → 进入课题详情页
  4. 在详情页查看 scope items 列表 → 点击 "添加" 输入 goal + done_contract → 提交后列表更新
  5. 点击 scope item 的 "完成" 按钮 → 该 item 标记完成，进度百分比和课题状态自动更新
  6. 点击 scope item 的 "删除" 按钮 → 二次确认后删除，进度和状态重算
  7. 点击 "暂停课题" 按钮 → 课题状态变更为 paused，scope item 操作禁用
  8. 点击 "恢复课题" 按钮 → 状态根据完成度重新计算
- **用户操作路径（Poller）**：
  1. 点击 "Poller" 标签或入口 → 展示 Poller 状态面板
  2. 查看当前状态标签（绿色 running / 黄色 paused）
  3. 点击 Pause → Poller 暂停，状态标签变黄，按钮变为 Resume
  4. 点击 Resume → Poller 恢复，状态标签变绿
  5. 点击 Trigger → Poller 触发立即 tick
- **系统反馈**：
  - 操作后列表/详情即时更新，不刷新整个页面
  - 错误信息以 banner 展示（后端错误或验证失败）
  - 删除操作需二次确认
- **状态变化**：
  - 课题创建/更新/删除后，列表自动刷新
  - Scope item 操作后，进度条和状态标签即时重算
  - Poller 控制操作即时生效，状态面板同步更新
- **异常/边界交互**：
  - 课题列表为空 → 空状态提示 + 创建引导
  - 暂停中的课题尝试操作 scope item → 展示错误提示
  - 操作因后端错误失败 → 展示具体错误信息和可能原因
- **不应发生的交互**：
  - Topic 操作不经用户确认直接删除
  - 会话切换后 Topic 数据不刷新
  - Poller 状态显示与实际后端状态不一致

## Acceptance Criteria / 验收标准

### 后端新增 Tauri 命令
- [ ] `list_topics` 命令已注册，支持可选 status 过滤
- [ ] `get_topic` 命令已注册，返回完整 Topic 结构
- [ ] `create_topic` 命令已注册，验证必填字段
- [ ] `update_topic` 命令已注册，支持部分更新
- [ ] `delete_topic` 命令已注册，返回删除结果
- [ ] `add_topic_scope_item` / `delete_topic_scope_item` / `complete_topic_scope_item` 已注册
- [ ] `pause_topic` / `resume_topic` 已注册
- [ ] `poll_status` / `poll_pause` / `poll_resume` / `poll_trigger` 已注册

### Topic 管理 GUI
- [ ] 课题列表展示完整（名称、状态标签、进度、更新时间）
- [ ] 支持按状态筛选
- [ ] 支持创建新课题（含名称和描述输入）
- [ ] 课题详情页展示完整信息
- [ ] Scope items 列表展示、添加、完成、删除
- [ ] 暂停/恢复课题操作
- [ ] 删除课题有二次确认
- [ ] scope item 操作后进度和状态自动重算

### Poller 管理 GUI
- [ ] Poller 状态面板展示当前状态、tick 数、任务数
- [ ] Pause / Resume / Trigger 按钮有效
- [ ] 状态变化即时反馈

## Constraints / 约束

- 业务约束：
  - 文档和代码冲突时，以文档为准，先同步文档再同步代码。
  - `Spec is Truth`，`No Spec, No Code`，`No Approval, No Execute`。
- 技术约束：
  - 新增 Tauri 命令仅限于对 Gateway 现有方法（`topic_store()`, `poll_status()`, `poll_pause()` 等）的封装。
  - GUI 不应绕过 Tauri 命令直接访问 Rust 后端 state / storage。
  - 前端技术栈保持 SvelteKit + Svelte 5 + `@tauri-apps/api`。
  - Topic 数据通过 Tauri 命令获取，不做前端缓存或状态管理库（Svelte 5 $state 管理即可）。
  - Topic 模块可复用现有 `src/lib/components/` 组件目录。
  - 前端 Topic 类型定义需对齐 Rust `Topic` / `ScopeInItem` / `PollerStatus` 结构。
- 时间/兼容性约束：
  - Poller 控制功能对普通用户透明，主要为技术支持/调试场景。
  - Topic 界面与聊天主工作台保持独立导航，不干扰核心聊天流程。

## Open Questions / 开放问题

- [x] Q1 Topic 管理界面作为独立页面还是侧栏面板？
  - 当前建议：侧栏面板标签页，与 providers/models/skills 同区域分页切换，不创建独立路由。
  - 用户回答：按需求文档走，不在这里确认。
  - 状态：待技术方案阶段确认

## Requirement Decisions / 需求决策

- 2026-07-30 17:43:
  - 决策：需求文档初稿创建，待用户确认。
