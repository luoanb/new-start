# Technical Plan / 技术方案: 神经元创建权重固定为 0

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-01_00-31_neuron-create-weight-zero/requirements.md`
- 需求确认状态：用户已确认「节点/边创建均为 0」
- 本方案覆盖：创建/连边强制置零、工具 schema、提示词、单测；并回写 bootstrap 需求中的过时表述

## Current Project Facts / 当前项目事实

- `ensure_creator_neuron`：已写 `weight: 0.0`（符合）
- `create_generated` / `create_generated_neuron` / `ensure_system_neuron`：落库使用 `draft.weight`（偏差）
- `CreateDownstreamNeuronTool`：接受 `weight`（默认 0）与 `edge_weight`（默认 1）（偏差）
- `create_downstream` / `create_downstream_neuron` / `link`：透传调用方权重（偏差）
- 默认种子与 user prompt：要求模型输出并打分 `weight`（偏差）
- 评价路径：`adjust_weight` / `adjust_connection_weight` 已存在，可保留

## Decision / 方案决策

- Selected：在 **Store 写入点 + Manager 业务入口** 双层强制 `0`；工具去掉参数；提示词去掉初始打分要求
- Why：单点漏网时 Store 仍兜底；契约对调用方更清晰

## API / 行为变更

1. `NeuronStore::create_neuron`：忽略 `create.weight`，插入恒为 `0.0`（或 Manager 统一改写后传入）。
2. `NeuronStore::create_downstream_neuron`：节点 `0.0`，边 `0.0`（忽略入参 `edge_weight`，或删除该参数并改调用方）。
3. `NeuronStore::link`：新建/覆盖连边时权重写 `0.0`（若需保留「绝对改边权」则另开管理 API；本迭代按需求：创建边=0，改权只用 delta）。
   - 细化：`link` 若被管理入口用于「建边」，强制 0；已有 `adjust_connection_weight` 负责增减。
4. `CreateDownstreamNeuronTool`：删除 `weight`、`edge_weight` 属性。
5. 自举 prompt / `DEFAULT_CREATE_NEURON_PROMPT`：JSON 示例可保留 `"weight":0` 说明忽略，或从必填说明中改为「可省略；系统强制 0」。
6. `generate_draft`：`weight` 非 finite 时不再因 weight 失败（或仍要求 finite 但忽略）；落库强制 0。
7. TUI `/neuron new`：已是 Default 0，保持；若有 link 带权命令，改为 0 或只建边。

推荐实现细节（最小改动）：

- Store `create_neuron` / `create_downstream_neuron` / `link` 写入常量 `0.0`。
- `create_downstream_neuron(..., edge_weight)` 可保留签名一版，但实现忽略 `edge_weight`；随后删工具参数即可。
- Manager 生成路径不再把 `draft.weight` 写入 `NeuronCreate`。

## Execution Steps / 执行步骤

1. 改 Store 三处写入强制 0；补单测。
2. 改 Manager 生成/ensure 落库与 prompt 文案；draft 校验放宽 weight。
3. 改 AI Tool schema 与 execute。
4. 回写 `neuron-bootstrap` requirements/technical-plan 中「创建可带 weight / 模型 weight 落库」的过时句。
5. `cargo fmt --check` / `cargo test`（`packages/agent-app/src-tauri`）。

## Risk And Mitigation / 风险与缓解

- 风险：冷启动大量新节点权重全 0，选一早期几乎全靠随机/LLM  
  - 缓解：符合产品意图；评价后再分化。
- 风险：`link` 强制 0 可能影响测试里用绝对边权搭场景  
  - 缓解：测试改用 `adjust_connection_weight` 或接受建边后 delta。

## Execute Checkpoint / 执行检查点

- 当前理解：创建节点/边权重一律 0；只许后续 delta 评价调整。
- 核心目标：堵住所有创建写权入口。
- 下一步 1–3 个动作：Store 强制 0 → Manager/Tool/prompt → 回写旧文档并跑测。
- 风险：早期候选同分随机增多（预期内）。
- 验证方式：单测断言创建权=0 + 忽略 draft/工具入参；`cargo test`。
