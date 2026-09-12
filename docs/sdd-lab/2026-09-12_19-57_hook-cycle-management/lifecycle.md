# Lifecycle / 生命周期: Hook 周期管理（流程决策升级）

```yaml
status: done
result: completed
created_at: 2026-09-12 19:57
updated_at: 2026-09-12 23:40
owner: user
```

## Current Summary / 当前摘要

- 批准状态：需求 + 技术方案已确认并**执行完成**（Step 1-7）
- 当前状态：**done / completed**
- 交付核心：`core/hook/cycle.rs`（周期字段派生 + 可调项契约 + `gate_check`）；`HookDef` 增 `group` / `disable_hint` / `cycle_params`；5 个 `run_*` 改为「派生 facts → 调用前判定 → 通过才调 handler」；6 个动作声明可调项且 handler 去判定；`config.json` `hooks` 节 + 启动覆盖 + 双写；命令/RPC `hooks_list` / `hook_set_enabled` / `hook_set_value`；面板「周期管理」由 `hooks_list` 驱动渲染
- 验证：`cargo check --all-targets` 0 错 0 警告；`cargo test --lib` **482 passed / 0 failed**；`pnpm --filter pulsar-app check` 0 errors / 20 warnings（基线）
- 下一步唯一动作：无（遗留可选项见下）

## Execution Log / 执行记录

- 1. 2026-09-12 19:57: 创建迭代。背景：用户提出「将流程决策升级为周期管理，控制 hook 的启停与触发时机」，要求先整理现状再出功能设计与技术方案。
- 2. 2026-09-12 19:57: 完成现状调研（`core/hook/defs.rs` 注册表能力 / 6 条装配 hook / 4 条休眠裁决 / run 内硬编码门控 / `hook_defs_list` 出参 / 前端只读面板 / config.json 持久化先例）；AskUserQuestion 拍板四项决策。需求与技术方案落盘，进入 planned。
- 3. 2026-09-12 20:10: 产出 `visual-design.md`（面板结构 / 控件 / 状态 / i18n / Icon 结论）。
- 4. 2026-09-12 20:30: 技术方案新增「可配参数矩阵」。
- 5. 2026-09-12 20:45: 收敛「不变量不纳入可配」与「简报/选型频率纳入可配」两项。
- 6. 2026-09-12 21:10: 抽象重构为「通用规范驱动」（`HookSpec` + `FactsProvider` + 框架统一门控）。
- 7. 2026-09-12 21:40: 补「选项通用性」（`FactSpec` 形态元数据 + 控件推导 + 开放 `group`）。
- 8. 2026-09-12 21:50: **用户指出概念混乱并纠正**——①`guard`/`params` 本不该拆成两个概念、也不该各配一套「提供方/消费方」机制；②「业务侧保留」指保留**功能效果**，具体配成什么值、怎么配，取决于**提供方给出哪些参数**，消费方只在其范围内取值。据此确认：**技术方案撤销**，机制层决策作废。
- 9. 2026-09-12 21:50: 按用户要求**先记录业务诉求**：`requirements.md` 重写（新增 §业务诉求清单 7 项、业务侧验收标准；机制层内容移出并作废）；状态 planned → draft。`visual-design.md` 系由已撤销的技术设计派生，**暂标为待重做**。
- 10. 2026-09-12 22:00: **确立周期模型抽象原则并落需求文档**。新增 §周期字段集（框架派生）：`mode` / `trigger` / 轮次计数 / 用户介入轮次计数 / 距上次用户介入的推进轮次 / 轮次来源 / 产物形态；注入点静态已知不作字段。三条口径经用户确认：①**计数源并存**（派生值仅服务周期条件，`topic.extra` 计数不动）；②**维度正交**（`轮次来源 × 产物形态`）；③**业务状态不进周期条件**（由 hook 内部自查）。并记录派生偏移约束（IP-1/IP-2 口径为「不含本轮」，「产物形态」类条件在 IP-1/IP-2 不可用）。O1/O2 关闭，O3~O6 待定。
- 11. 2026-09-12 22:15: **对 7 条业务诉求做「派生字段是否够用」的收束对照**，发现并补齐两处：①**G1** 缺「上一轮产物形态」→ 产物形态改为**可指定轮次（本轮 | 上一轮）**，派生式 = `messages` 末条是否 `role == Tool`（覆盖 IP-1/IP-2 场景，如简报刷新的「上轮非工具结束」）；②**G2** 「是否压缩」**不属周期**——压缩每轮都调用、内部按阈值检测。并落一条同源推论：周期性动作内部的**多条件决策**（如简报刷新的「内容变化」）留在 hook 内，周期只提供频率/形态输入，不把业务态 OR 进周期条件。O3 收窄为「第 3、6 项待确认」，业务诉求 #4/#7 行与验收标准同步更新。
- 12. 2026-09-12 22:30: **重做技术方案并落盘**（`technical-plan.md`）。设计要点：①`core/hook/cycle.rs`（新）承担周期字段派生（`CycleField` / `CycleFacts::derive`，口径遵循需求 §派生口径约束）与可调项契约（`CycleParamSpec`：字段 + 形态 + 默认 + **用途 `CallGate`/`Internal`**）与 `gate_match`；②`HookDef` 增 `group` / `protected` / `cycle_params`，注册表 5 个 `run_*` 改为「派生 facts → 调用前判定 → 通过才调 handler」，handler 去判定；③**一个概念 + 一个属性**（所有可调项都是「周期参数」，`usage` 决定调用判定 or 动作内部读取），不再拆 guard/params；④持久化 `config.json` 的 `hooks` 节（`enabled` + `values`）；⑤命令 `hooks_list` / `hook_set_enabled` / `hook_set_value`；⑥面板按声明渲染（零领域硬编码）。状态 draft → planned。剩余 Q1~Q5 待用户拍板。
- 13. 2026-09-12 22:45: **Q1~Q5 全部拍板**：Q1 = A（框架调用前统一判定）；Q2 = C（声明式预设项，不做字段自由组合）；Q3 = **全部动作均可调**（第 3、6 项暴露「模式窗口」；`compaction` 仅启停）；Q4 = **取消硬保护**（不设 `protected`，改为 `disable_hint` 风险提示，不阻止关停）；Q5 = **统一管理**（一个面板列全部动作，沿用升级现有 `hook-judgements` 视图）。据此同步改写技术方案（Decision / `HookDef.disable_hint` / 命令语义 / §API-4 表 / 风险 / 检查点）与需求文档（业务诉求 #3/#6 可调、启停统一、O3~O6 关闭、Requirement Decision）。技术方案 Open Questions 全部关闭，**等待用户批准执行**。
- 14. 2026-09-12 23:40: 用户「开始执行」→ 完成 Step 1-7 全部实现并验证：
  - Step 1 `core/hook/cycle.rs`（周期字段 + 可调项契约 + `gate_check` + 单测）；Step 2 `defs.rs`（`HookDef` 扩展 / `param_of` / `set_value` / `snapshot_all` / 5 个 `run_*` 调用前判定 / skip 日志 `PHASE_HOOK_CYCLE_GATE`）；Step 3 六个动作声明可调项 + handler 去判定 + `AssistantSession` 持 `Arc<HookRegistry>`；Step 4 `infra/config.rs` `HooksSection` + `Gateway` 启动覆盖与双写；Step 5 命令 + RPC 三接口；Step 6 前端（types / contracts / 面板「周期管理」分区渲染 / i18n / views 重命名）；Step 7 回归验证 + 活文档回写。
  - **落地偏差（Reverse Sync）**：`user-round-judgement` 的复核节奏未放入 `CallGate`（因需与「未绑定课题必跑」业务状态取 OR），改为 `Internal(review_every_n)` + handler 内判定；已回写技术方案 §API-4。
  - 验证：`cargo check --all-targets` 0 错 0 警告；`cargo test --lib` 482 passed / 0 failed；`pnpm --filter pulsar-app check` 0 errors / 20 warnings（基线）。
  - 回写：`technical-plan.md`（落地结果 + 偏差）、`docs/pulsar/hook/index.md`（§8b 周期契约 + 索引）。
  - 遗留（可选项）：`docs/pulsar/architecture.md` 未逐字更新（`hook/index.md` 已补周期契约）。
- 15. 2026-09-12 23:55: **面板布局优化**（用户反馈「太丑」）：动作区由「单行塞满标签/分组/注入点/状态 + 常显风险提示 + 卡片边框」改为——两行结构（名称 / 分组·注入点）＋ hairline 行分隔（去卡片）＋ 仅在**关停时**显示风险提示 ＋ 展开参数按用途分组（`触发条件` / `策略参数`）与宽控件上下布局；新增 i18n `cycle.noParams`。`pnpm --filter pulsar-app check` 0 errors。
- 16. 2026-09-13 00:10: **UI 迭代二**（用户反馈：①开关挪到展开面板 ②很多字段多语言没配齐）：
  - **i18n 修复**：Rust 侧 key 由点号命名（`cycle.param.mode` / `cycle.group.shell` / `cycle.hint.roundBefore`）统一改为**扁平命名**（`cycle.paramMode` / `cycle.groupShell` / `cycle.hintRoundBefore`），与 `translations.ts` 的 `cycle.*` 扁平键对齐；4 个业务动作的 `label` 由中文硬编码改为 i18n key（新增 `cycle.hookRoundBefore` / `hookRoundAfter` / `hookSelectNeuron` / `hookCompaction`，en + zh）。
  - **开关位置**：启停 Toggle 从折叠行移入**展开面板**（折叠行只留名称 / 分组·注入点 / 停用标记 / chevron，整行可点展开）；风险提示随开关移入展开面板；新增 i18n `cycle.enabled`。
  - 验证：`cargo check --all-targets` 0 错 0 警告；`cargo test --lib` 482 passed；`pnpm --filter pulsar-app check` 0 errors。

## Transition Log / 状态变化

- draft → planned（2026-09-12 19:57）：需求边界经四项决策收敛、技术方案成形。依据：AskUserQuestion 四项确认。
- planned → draft（2026-09-12 21:50）：
  - 变更前：planned
  - 变更后：draft
  - 变更原因：**技术方案被撤销**（机制层抽象被用户否定）；需求语义回到「业务诉求 + 提供方参数」层面，尚未重新对齐
  - 依据：用户明确「技术方案已经撤销，先把业务诉求记录到需求文档」
  - 下一步动作：用户确认 `requirements.md`（§业务诉求清单 + O1~O6）→ 重做技术方案
- draft → planned（2026-09-12 22:30）：
  - 变更前：draft
  - 变更后：planned
  - 变更原因：周期字段集与抽象边界已定；技术方案重做完成（`technical-plan.md`）
  - 依据：用户「开始出技术方案」
  - 下一步动作：用户就技术方案 Q1~Q5 拍板并批准方案 → 进入 executing
- planned → executing（2026-09-12 23:00）：
  - 变更前：planned
  - 变更后：executing
  - 变更原因：Q1~Q5 全部拍板，用户下达「开始执行」
  - 依据：用户「开始执行」
  - 下一步动作：按 Step 1-7 实现并验证
- executing → done（2026-09-12 23:40）：
  - 变更前：executing
  - 变更后：done / result: completed
  - 变更原因：Step 1-7 全部完成，验证通过（`cargo check --all-targets` 0 错 0 警告；`cargo test --lib` 482 passed；`pnpm check` 0 errors）
  - 依据：编译与测试结果 + 活文档回写完成
  - 下一步动作：无（遗留可选项：`visual-design.md` 按新契约重做；`docs/pulsar/architecture.md` 逐字更新）

## Resume Anchor / 恢复锚点

- 恢复阅读顺序：本文件 → `requirements.md` → `technical-plan.md`（含「落地结果」）
- 未关闭项：无（需求 O1~O6 与技术方案 Q1~Q5 全部关闭）
- 遗留（可选）：`docs/pulsar/architecture.md` 未逐字更新（`hook/index.md` 已补周期契约）；`visual-design.md` 已删除，不再适用
