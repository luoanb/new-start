# Requirements / 需求文档: Hook 裁决纠偏与透明化

## Restated Understanding / 需求复述

- 我理解当前需求是：修复 4 个裁决 hook（assistant_complete_scope / match_topic / revise_topic / score_feedback）在模型偶发输出非 JSON 时 `.await?` 硬上抛、阻断整轮主对话的问题（事故实例：`conv_1787253076882845861` 在 complete_scope 裁决收到散文后 `send_model_message_stream failed` 导致整轮报错）。同时裁决过程目前全程不落库、不可观测，失败只能翻 8MB 日志。
- 当前核心目标是：**hook 级统一纠偏**（结构化输出预防 + 有限重试自愈 + 失败中性降级不阻断主轮次）+ **每次裁决全量落库** + **用户可见**（独立全局「Hook 判定」面板 + 消息列表就地裁决卡，AI 侧裁决透明不黑盒）。
- 当前边界是：纠偏机制挂在统一入口 `call_judgement` / hook 基线上，不区分具体 system_type；只影响裁决类调用，不动主对话管道；裁决记录不写入主对话消息列表（独立存储）。
- 暂不处理：手动重跑裁决；hook 调用超时隔离与异步化；消息旁迷你常驻徽标；json_schema 用于主对话；历史裁决数据回填。

## Scope / 范围

- In:
  - hook 级统一纠偏（A 失败降级 + B 有限重试 + C 结构化输出，见 technical-plan 方案对比与决策）
  - 裁决全量持久化（新表 `hook_judgements`）
  - 独立全局面板「Hook 判定」（与「会话/文件」同级的 sidebar view），支持时间线列表、过滤、详情展开、定位到会话内锚点消息
  - 消息列表内联裁决卡：锚点消息下方展示执行进度与过程（裁决中 → 终态着色），可展开完整输入/输出/决策
  - 数据通道：`hook_judgements_list(filters)` / `hook_defs_list()` tauri command + rpc 分发；裁决开始/结束两阶段事件推送
  - i18n：面板标题、状态标签、字段名中英 key
- Out:
  - 手动重跑裁决入口
  - hook 调用超时隔离 / 异步化（E 方案）
  - 消息旁迷你常驻徽标（裁决卡即展示）
  - json_schema 约束扩展到主对话调用
  - 历史裁决数据回填

## User Interaction / 用户交互

- 触发入口：
  - 裁决自动触发：会话推进过程中 after/before hook 产生裁决（用户无需操作）
  - 面板入口：sidebar 新增「Hook 判定」view（与会话/文件同级）
  - 消息卡入口：主消息列表锚点消息下方自动渲染裁决卡
- 用户操作路径：
  - 面板：打开「Hook 判定」→ 查看跨会话时间线 → 按 hook_type / status / conversation 过滤 → 点击记录展开详情（payload / raw / decision / error / attempts / duration）→ 「在会话中定位」跳转到对应会话并滚动高亮锚点消息
  - 消息卡：锚点消息下看到裁决卡（徽标 + 进度 + 结果摘要）→ 点击展开完整过程
- 系统反馈：
  - 裁决卡实时进度：裁决中（spinner）→ 终态（✓ 完成 / ⚠ 降级 / ✕ 失败），降级时展示原因摘要（如「模型输出非 JSON」）
  - 面板记录随裁决产生即时出现（事件驱动），重启后历史记录照常渲染
- 状态变化：
  - 裁决终态：`ok` / `retried_ok` / `downgraded`（降级原因见 error；重试次数、耗时随记录展示）
- 异常/边界交互：
  - 模型输出非 JSON：自动重试 1 次（带失败反馈）→ 仍失败则中性降级 + warn，主轮次不报错
  - 面板无记录时显示空态提示
- 不应发生的交互：
  - 裁决失败导致主轮次报错/中断
  - 裁决过程对用户不可见（黑盒）
  - 裁决记录混入主对话消息数组（独立存储，仅以附属块渲染）

## Acceptance Criteria / 验收标准

- [ ] 4 个裁决 hook 在模型输出非 JSON 时不再阻断主轮次：重试 1 次仍失败 → 中性降级 + warn 留痕（complete_scope 空判定 / match_topic 不创建不切换 / revise_topic 不修订 / score_feedback 跳过打分）
- [ ] 裁决调用带结构化输出约束：json_schema 契约 → 不支持则 json_object → 再不支持无约束（能力探测 + 按 provider/model 缓存）
- [ ] 每次裁决（成功 / 重试后成功 / 降级 / 失败）全量落库 `hook_judgements`，字段完整（含锚点、payload、每轮尝试原文 attempts_detail、最终 raw、decision、status、error、attempts、duration）
- [ ] sidebar 新增「Hook 判定」面板：时间线列表 + hook_type/status/conversation 过滤 + 详情展开 + 「在会话中定位」跳转
- [ ] 主消息列表锚点消息下方渲染裁决卡：进度（裁决中→终态）+ 结果摘要 + 展开完整过程；运行中实时更新，重启后照常渲染
- [ ] 裁决卡为锚点消息附属渲染块，不插入消息数组、不影响 message_index 与虚拟滚动
- [ ] i18n 中英 key 完整
- [ ] `cargo check` + `cargo test`（新增纠偏降级、能力探测、记录写入与锚点解析测试）+ `pnpm --filter pulsar-app check` 0 error；App 内面板与消息卡可见并可用

## Constraints / 约束

- 业务约束：
  - 裁决失败只影响副作用本身，不丢失本轮模型产物（与 run_round 既有注释语义一致）
  - AI 侧裁决过程对用户透明可见
  - 全量保留裁决记录，无清理策略
- 技术约束：
  - 存储范式对齐 `topic_store`（`conn: Arc<Mutex<Connection>>` + `on_change: StateEmitter` + `init_table` + 迁移函数 + 统一 `emit_change`）
  - `response_format` 经现有 `extra` 扁平透传通道下发（openai_compat 协议层零改动）
  - 裁决调用走 `run_raw_round`（无 coordinator.begin），重试安全
  - 面板查询走独立轻量查询，不混入消息列表拉取
  - 不引入新外部依赖
- 时间/兼容性约束：
  - 结构化输出能力按 provider/model 白名单缓存探测结果，兼容不支持 response_format 的网关

## Open Questions / 开放问题

- [x] Q1 纠偏方向（已关闭）：用户选定 A 失败降级 + B 有限重试 + C 结构化输出（跳过 D 修复管线、E 超时异步化）
- [x] Q2 面板位置与形态（已关闭）：与「会话/文件」同级的独立 sidebar view「Hook 判定」，统一查看所有记录；不提供手动重跑
- [x] Q3 保留策略（已关闭）：全量保留
- [x] Q4 用户侧可见性（已关闭）：主消息列表也要能看到裁决执行进度和过程（AI 侧透明）→ 内联裁决卡（形态/交互经用户确认后写入方案）

## Requirement Decisions / 需求决策

- 2026-08-22:
  - 决策：纠偏采用 A（失败降级，中性默认值 + warn 不阻断）+ B（解析失败带反馈重试 1 次）+ C（结构化输出 json_schema → json_object → 无约束的能力探测降级链）组合；不做 D（解析修复管线）、E（超时/异步化）
  - 原因：C 源头预防格式、B 偶发跑偏自愈、A 失败兜底，三者互补；D/E 成本高或超出本期范围
  - 决策：新增与「会话/文件」同级的独立面板「Hook 判定」，统一查看所有裁决记录；不提供手动重跑；记录全量保留
  - 原因：裁决过程需要统一可观测入口；用户明确不重跑、要全量
  - 决策：主消息列表锚点消息下方渲染内联裁决卡，展示执行进度与过程，可展开完整输入/输出/决策
  - 原因：用户要求用户侧消息列表可见裁决执行进度和过程，AI 侧保持透明
