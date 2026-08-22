# Lifecycle / 生命周期: Hook 裁决纠偏与透明化

```yaml
status: done
result: ok
created_at: 2026-08-22 11:10
updated_at: 2026-08-22
owner: user
```

## Current Summary / 当前摘要

- 批准状态：技术方案已批准，Step 1-7 全部执行完成并通过检查（后端 364 测试全绿、前端 svelte-check 0 errors）
- 当前状态：done（A+B+C 统一纠偏 + 裁决全量落库 + 独立「Hook 判定」面板 + 消息内联裁决卡）
- 下一步唯一动作：人工验证——用弱模型（如 flash）触发散文输出，确认主对话不中断、面板/消息卡出现降级记录

## Execution Log / 执行记录

- 1. 2026-08-22 11:03: 事故复盘定位（`conv_1787253076882845861` complete_scope 散文输出 → 硬上抛 + busy 泄漏）；busy 泄漏已单独修复（RoundGuard）。
- 2. 2026-08-22 11:10: 按用户要求以 sdd-lab 规范重建迭代文档（原 micro-spec 方案表达力不足，已废弃删除）；需求对齐（A+B+C、面板、全量保留、不重跑、消息卡）与方案落盘完成。
- 3. 2026-08-22: `technical-plan.md` 落盘（A+B+C 组合、`hook_judgements` 表、两阶段 `HookJudgements` 事件、`hook_judgements_list` / `hook_defs_list` command、response_format 能力探测降级链）；核实代码事实并删除废弃 micro-spec 文件。状态保持 planned，等待用户批准后进入 executing。
- 4. 2026-08-22: 技术方案细化经多轮讨论确认：①response_format 是每个 hook 自带的属性，schema 就近定义；②hook 概念收拢为 `HookDef` 静态清单（策略 5，新文件 `core/hook.rs`），收拢 system_type / label / response_format / neutral_fallback，`call_judgement` 签名改收 `&HookDef`；③response_format 注入路径与 `thinking_override` 对称（run_raw_round 新参数 → providers 注入 `req.extra`）。已写入 technical-plan 的 API Design、Decision 与执行步骤（Step 2 新增，后续顺延为 Step 2-7）。
- 5. 2026-08-22: 方案审核修复——①终态删 `parse_failed` 改三态（`ok` / `retried_ok` / `downgraded`，同步 requirements）；②补 `JudgementOutcome` 返回契约与 `JudgementAnchor` 锚点参数（`call_judgement` 落库所需）；③补 `hook_defs_list` command 解决前端 label 映射；④表加 `attempts_detail` 保证重试两轮原文全量保留；⑤Contract Scope 变更类型改「新增 + 扩展」；⑥Execute Checkpoint 步骤数改 Step 1-7。
- 6. 2026-08-22: 用户批准 technical-plan，进入 executing；误按 sdd-exec-scheme 规范产出 `exec-scheme-bridge.md`（三段桥接设计）。桥接契约补充 6 项执行细节：`ResponseFormatSpec` wire 形态、`ModelCallRequest.response_format` 载体、锚点索引公式 `round.startIndex + mi`、match_topic 降级值 `"none"`、`run_raw_round` Err 也降级、schema 就近定义于 hook.rs。
- 7. 2026-08-22: 用户纠正：「sdd-lab 流程应产出 visual-design.md（页面设计文档），不是 sdd-exec-scheme 桥接文档」。已按 templates.md 模板 + git-support 先例格式产出 `visual-design.md`：①sidebar「Hook 判定」面板（过滤工具条 + 时间线倒序列表 + 详情展开 + 在会话中定位 + 空态）；②消息列表内联裁决卡（锚点附属渲染块 + pending/终态两阶段 + 展开详情 + 降级提示）；四态 token 映射表（pending/ok/retried_ok/downgraded）+ icon 导出规范。`exec-scheme-bridge.md` 标记为误建产物（其代码事实已并入 technical-plan 与 visual-design 依据），待用户确认是否保留。下一步：用户确认 visual-design.md → Step 1 编码。
- 8. 2026-08-22: Step 1-7 全部执行完成。实际改动摘要：
  - **Step 1 存储层**：新增 `core/hook_judgement_store.rs`（`hook_judgements` 表 17 字段 + `insert_start`/`finish`/`get`/`list` 两阶段写入 + `HookJudgements` 变更广播；全量保留 raw_response/attempts_detail 原文）；`new_hook_judgement_id()`。修复 2 个关键 bug：`get()` SQL 缺 `WHERE id = ?1`；`finish()` 持写锁同线程重复加锁死锁（写锁限定独立作用域主动释放）。
  - **Step 2 Hook 收拢层**：新增 `core/hook.rs`（`HookDef`/`HookDefMeta`/`JudgementStatus` 三态/`AttemptRecord`/`JudgementOutcome`/`JudgementAnchor` + 4 个 schema 常量 + 4 个 neutral_fallback + `HOOK_DEFS` 静态清单 + `hook_defs_meta()`）；`ResponseFormatSpec::JsonSchema` 改 `Cow<'static, str>` + derive serde（同步 hook.rs/providers.rs 消费点）。
  - **Step 3 统一纠偏**：`call_judgement` 重构（`def: &HookDef` + `JudgementAnchor` 入参；C 结构化输出预防 → B 带反馈重试 1 次 → A 中性降级兜底；两阶段落库；`JudgementOutcome` 返回）；`run_raw_round` 新增 `response_format` 参数（透传 chain：run_raw_round → execute → call_model → ModelCallRequest → providers `apply_response_format`）；`structured_output_support()` 能力探测降级链（json_schema → json_object → 无约束，按 provider/model 缓存）；4 个 hook 调用点全部改走统一入口；新增 4 个验收单测（retry→retried_ok / 双败→downgraded / 单次→ok / 降级链）。
  - **Step 4 数据通道**：`StateChange::HookJudgements` 变体 + 序列化测试；lib.rs 新增 `hook_judgements_list`/`hook_defs_list` command + 注册 invoke_handler；RPC 分发同步（`net/rpc.rs` 两个分支 + `with_hook_judgement` helper）。
  - **Step 5 前端面板**：`HookJudgementPanel.svelte`（时间线列表 + hook_type/status 过滤 + 详情展开 + 在会话中定位）；`views.ts` 注册 `hook-judgements` view；`layoutTypes.ts` 默认 sidebar 追加；`LayoutStore` 新增 `locateAnchor` 定位机制；`ChatArea` 消费锚点滚动高亮（`.message[data-message-index]` + locate-flash 动画）。
  - **Step 6 消息内联裁决卡**：`JudgementCard.svelte`（pending spinner → 终态徽标 ✓/↻✓/⚠ + 结果摘要 + 展开完整输入/输出/决策）；ChatArea 旁路渲染（`anchor_message_index` 匹配，不插入消息数组、不影响虚拟滚动）；按会话拉取 + `hook_judgements` 事件实时刷新。
  - **Step 7 i18n**：新增 `views.hookJudgements`、`judgement.*`（状态/字段/空态/过滤/定位）、`hook.*`（4 个 hook 标签）zh/en key。
- 9. 2026-08-22: 验证结果——`cargo check --lib` 通过；`cargo test --lib` 364 passed / 0 failed（含 4 个新裁决单测与 hook_judgement_store 3 个单测）；`pnpm --filter pulsar-app check` svelte-check 0 errors / 20 warnings（均为既有代码警告，无新增）。下一步状态：等待用户人工验证（弱模型触发散文输出 → 主对话不中断 + 面板/消息卡出现降级记录）。
