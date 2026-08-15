# Lifecycle / 生命周期: Neuron Unified Management

```yaml
status: done
result: completed
created_at: 2026-08-10 15:30
updated_at: 2026-08-11 00:20
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已确认并完整执行
- 当前状态：done（requirements + visual-design + technical-plan + 全量实现 + 验证通过）
- 交付内容：神经元统一管理——info 容器《神经元》列表（分页/搜索/类型筛选/多选/编辑/创建/发起），主区《神经元》画布降级为子页面（数据源改列表选中项，含多选核心），`NeuronDetailDrawer` 扩展系统类型绑定/换绑/取消（二次确认）+ 行为管理控件，移除 `session-specs` 面板与入口，布局 v8→v9 迁移
- 验证：`cargo test --lib` 165 passed、`svelte-check` 0 errors

## Execution Log / 执行记录

- 1. 2026-08-10 15:30: 创建迭代 `neuron-unified-management`，生成 `requirements.md`（draft）。
- 2. 2026-08-10 16:10: 用户确认 Q1（全部/系统/普通筛选）、Q2（沿用多选机制）、Q3（后端分页）、Q4（移除 session-specs 面板），需求文档开放问题全部关闭。
- 3. 2026-08-10 16:30: 按用户确认的设计模拟 + 项目设计规范（app.html tokens / ViewContainer tab / system_type 色板机制），生成 `visual-design.md`。
- 4. 2026-08-10 16:40: 核对后端与前端现状后，生成 `technical-plan.md`（planned）。
- 5. 2026-08-11 00:00: 后端完成（b1-b4）：`NeuronStore::list_neurons_page`（分页/搜索/类型筛选，LIKE 通配符转义）、`NeuronPage`/`NeuronKindFilter` 模型、`NeuronManager` 转发 + 系统类型唯一约束预检查、`SessionSpecManager::update_behavior_for_admin` 校验放宽为「需系统神经元」；lib.rs 移除 `list_session_specs`/`create_session_spec`/`update_session_spec_behavior`，新增 `list_neurons_page`/`set_neuron_system_type`/`update_neuron_behavior`；新增 3 个单测（分页搜索筛选 / 绑定换绑取消 / 行为需系统类型）。`cargo test --lib` 165 passed。
- 6. 2026-08-11 00:10: 前端布局（f1）：`layoutTypes` 升 v9（info 默认视图含 `neurons-list`、`MainPanelType` 移除 `session-specs`）、`layoutStorage` v8→v9 迁移（info 补 `neurons-list`、main 清理 `session-specs` 面板）、`views.ts` 注册 `NeuronListPanel` 并移除 `session-specs`。
- 7. 2026-08-11 00:15: 共享状态与列表（f2-f3）：dataStore 移除 `sessionSpecs` 状态与 3 个系统神经元 action，新增 `neuronSelection`/`neuronSelectionMode`/`neuronEditRequestId`/`neuronCreateRequest`/`neuronLaunchRequestId` 与共享 actions；新建 `NeuronListPanel.svelte`（搜索防抖、类型筛选、滚动加载、多选开关、行点击/编辑/发起、创建入口）。
- 8. 2026-08-11 00:18: 画布与编辑（f4-f5）：`NeuronManager` 移除搜索与核心下拉，seed 改由列表选中项驱动（画布内切换写回共享状态，多选 append），创建/编辑请求经共享状态转发；`NeuronDetailDrawer` 扩展系统类型绑定交互（bind/rebind/unbind + 二次确认弹层）与行为管理区块；`BehaviorFields.svelte` 自 SessionSpecsPanel 抽离为受控组件。
- 9. 2026-08-11 00:20: 移除旧入口（f6）：删除 `SessionSpecsPanel.svelte`、`SessionCreateModal` 移除「按系统神经元发起」卡片、+page.svelte 移除 `openSessionSpecs`；i18n 新增 `neuronListPanel`/`neuronEditor` 区块并移除 `sessionSpecsPanel`/`createModal.bySpec`/`views.sessionSpecs`/`neuronPanel.coreSelect`。验证：`svelte-check` 0 errors。
