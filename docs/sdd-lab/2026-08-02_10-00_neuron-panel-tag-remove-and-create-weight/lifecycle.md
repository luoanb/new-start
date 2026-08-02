# Lifecycle / 生命周期: 神经元面板 — 移除 tag 筛选 + 创建/权重

```yaml
status: done
result: success
created_at: 2026-08-02 10:00
updated_at: 2026-08-02 11:45
owner: user
```

## Current Summary / 当前摘要

- 批准状态：需求与技术方案已确认，实现完成并验证
- 当前状态：done（已交付）
- 交付内容：删除神经元左侧列表（含其顶部 tag 筛选）；新增「创建神经元（孤立/下游）」与「调整权重（自身/边）」；后端新增 1 个 pub 方法 + 3 个 Tauri 命令

## Execution Log / 执行记录

- 1. 2026-08-02 10:00: 按 sdd-lab 规范重建迭代文档，删除误建于需求阶段的 `technical-plan.md`，重写 `requirements.md`，新建 `lifecycle.md`；状态 draft。
- 2. 2026-08-02 10:35: 需求确认后进入 planned；生成 `technical-plan.md`；状态 planned。
- 3. 2026-08-02 10:40: 用户确认执行，状态转 executing。
- 4. 2026-08-02 11:20: 首轮实现：后端 `create_plain` + 3 命令 + 注册；前端去 tag 筛选（误删为中间工具栏 tag，未删左侧列表）、创建弹窗、抽屉权重步进、i18n 补键。编译通过。
- 5. 2026-08-02 11:45: **理解纠正**：用户确认需求为「不要展示左侧列表本身（含其顶部 tag 筛选）」，而非仅删中间工具栏 tag。修正实现：
  - 删除 `NeuronManager.svelte` 中 `.sidebar` 的 `<NeuronIndex>`（左侧按 system_type 分组的列表）及其顶部 tag 筛选。
  - 移除 `NeuronIndex` 导入、`linkCounts` 状态与计数逻辑、`.sidebar` 样式；`.body` grid 改为单列（图占满）；清理媒体查询。
  - 保留 `filteredNeurons`（现仅用于图按 search 过滤）。「创建神经元」入口保留在中间工具栏（搜索框右侧）。
  - `svelte-check` 0 errors / 39 warnings（均仓库既有）。状态维持 done。

## Requirement Correction Note / 需求纠正说明

- 原需求「神经元左侧的列表，顶部的tag筛选都去掉」最终解读为：**删除左侧列表组件 NeuronIndex 及其顶部 tag 筛选**（不是删除中间图区工具栏的 tag）。首轮误删位置已在 11:45 修正。
- 顶部 tag 筛选与左侧列表同属 `NeuronIndex.svelte`（嵌入 `NeuronManager` 的 `.sidebar`），二者一并移除。
