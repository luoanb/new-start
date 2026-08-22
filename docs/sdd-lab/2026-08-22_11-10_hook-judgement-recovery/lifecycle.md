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
- 10. 2026-08-22: 设计优化（用户反馈：面板与其他面板不搭、消息裁决卡样式需参考轮询推荐等内联块）：
  - **HookJudgementPanel** 对齐项目面板词汇：新增 `panel-toolbar`（panel-title + icon-btn 刷新）；过滤条改 `filter-bar` 紧凑容器（surface 底 + 圆角）+ 结果计数；时间线条目重构（短时间戳 HH:mm:ss + hook 徽标 mono/elevated 底 + 终态徽标带前置圆点 + 决策/错误摘要 truncate + toggle-icon chevron）；卡片 `--color-bg` + `radius-md`；状态徽标硬编码 rgba 改 `color-mix(in oklch, var(--color-*) 12%, transparent)` 语义 token；空态区分「暂无记录 / 无匹配记录」。
  - **JudgementCard** 对齐消息区内联块族（NudgeBlock/ThinkingBlock/ToolResultBlock）：混合底色 `color-mix(surface 45%, bg)` + 淡边框 + 左侧 3px 语义色 accent（ok=success / retried_ok=primary / downgraded=warning / pending=muted）+ `radius-md`；折叠条 `summary` + `toggle-icon` chevron 词汇；状态色 token 化；pending 补动态耗时（`裁决中 · <hook>` + 从 created_at 实时计时）；悬停语义 tooltip（title）。
  - **ChatArea**：`refreshJudgements` 并行拉 `hook_defs_list`，为内联卡提供 `hookLabel`（i18n 短名）。
  - **i18n**：新增 `judgement.running` / `noMatch` / `refresh` / `tooltip.*` zh/en key。
  - **需求收敛**：裁决记录无会话标题字段（仅 conversation_id），面板不做会话搜索框，摘要列显示决策/错误摘要（用户确认）。visual-design.md 已同步。
  - 验证：`pnpm --filter pulsar-app check` svelte-check 0 errors / 20 warnings（均为既有警告）。
- 11. 2026-08-22: 设计收敛（用户反馈：工具类应用需克制，上一轮装饰过火）。收敛点：
  - **JudgementCard**：去掉左侧 3px 语义色 accent 竖条与混合底色，回归中性 `--color-surface` + 淡边框 + `radius-sm`；状态字符改小号非加粗，语义色仅此一处；无动画。
  - **HookJudgementPanel**：状态徽标去掉前置圆点 / color-mix 彩底 / pending 闪烁动画，改为小号文字 + 语义色文字色；保留中性结构（panel-toolbar / filter-bar / hook-badge elevated 底）。
  - visual-design.md 视觉基线已同步。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 12. 2026-08-22: 消息内对齐（用户反馈：对话消息里裁决卡左右边距与字体颜色需与消息正文对齐）：JudgementCard 左右 `margin` 改为 `--space-4`（对齐 `.message` 的左右 padding）；正文信息（状态文字 / 决策摘要）用 `--color-text` 与消息正文一致，`--color-text-muted` 仅保留给耗时时间戳。visual-design.md 已同步。
- 13. 2026-08-22: 面板徽标统一收敛（用户反馈：其他课题/轮询面板卡片也需对齐统一视觉规范）：**TopicPanel** 状态徽标去掉 color-mix 彩底与前置圆点，**PollerPanel** 状态徽标去掉实底白字，统一改为「小号文字 + 语义色文字色」（与 HookJudgementPanel 收敛后规范一致）。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 14. 2026-08-22: 消息区卡片全量统一（用户反馈：都是卡片，样式必须对齐）：5 个消息区内联块（JudgementCard / NudgeBlock / ThinkingBlock / ToolCallBlock / ToolResultBlock）统一为同一套规范——`--color-surface` 底 + `--color-border` 淡边框 + `--radius-sm`，无 accent 竖条、无混合底色、无装饰动画；折叠条 `summary` 整行按钮（padding `space-1 space-2`、fs-xs）+ 右侧 `toggle-icon` chevron + CopyButton；正文信息用 `--color-text`，类型标签/元信息用 `--color-text-muted`；展开详情 `border-top` 分隔 + padding `space-2`。同时为 ToolCall/ToolResult 补上卡片容器、去掉 stderr 左侧竖条改红色文字。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 15. 2026-08-22: 裁决信息层级重排 + 面板徽标清理（用户确认：折叠行只锚定类型，展开第一眼必须看到结果）：**JudgementCard** 折叠行收敛为「hook 类型名（muted 纯文字）+ chevron」，删除折叠行上的 spinner/状态字符/摘要/耗时；展开区顶部新增 `.verdict` 结果行（状态字符 + 语义色状态文字 + pending 动态耗时），其后是决策依据（decision/error），再才是元信息与详情折叠块。**HookJudgementPanel**：hook 类型徽标去掉 `--color-elevated` 灰底改纯文字；zh hook label 去掉「裁决」尾巴（范围完成裁决→范围完成 等 4 个），en 本就无后缀不动。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 16. 2026-08-22: 收敛修正（用户反馈两处）：① 消息卡折叠行主体（hook 名）改回正文色 `--color-text`，与其他卡片折叠行主体对齐（不用 muted）；② 恢复 pending 执行中指示——折叠行 `pending` 时显示「静态 ◌ + hook 名 + 动态耗时」（muted，无动画），保证执行进度折叠行实时可见；终态折叠行仍只显示 hook 名。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 19. 2026-08-22: 执行工具卡片折叠行主体颜色同步（用户要求「执行工具的卡片也同步一下设计」）：**ToolCallBlock**（🛠 工具调用）与 **ToolResultBlock**（🖥 工具结果）折叠行 label（工具名，卡片主体信息）从 `--color-text-muted` 改回正文色 `--color-text`，与简报 preview / 裁决卡 hook 名一致。区分：折叠行**主体信息**（工具名 / 简报摘要 / hook 名）= `--color-text`；**类型标签 / 元信息**（🧠 思考过程 label、call-id、section-label、耗时等）= `--color-text-muted`。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 20. 2026-08-22: 折叠行布局与图标统一（用户反馈：「复制icon和>的位置呢？卡片高度呢？？那个工具执行和返回的icon能不能设计一下，太丑」）：① **JudgementCard** 补齐 CopyButton（复制完整裁决记录 JSON，含决策/错误/明细），折叠行由 `<button>` 改为 div `role="button"` + onkeydown（Enter/Space），`.summary` 补 `width:100%` + `border-radius: --radius-sm`，与其余 4 卡结构完全一致；② **折叠行统一布局** = `主体文字(flex:1 truncate) → CopyButton → chevron`，CopyButton 26px 定高即折叠行高度基准（上下 padding `space-1` → 总高 34px 全卡一致）；ToolCallBlock / ToolResultBlock 的 `.label` 补 `flex:1 + min-width:0`，修复长工具名不截断问题；③ **工具卡 icon 去 emoji**：ToolCallBlock 🛠 → lucide `terminal` 14px stroke SVG、ToolResultBlock 🖥 → lucide `monitor` 14px stroke SVG（stroke currentColor，图标 muted 装饰色，主体工具名正文色）。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 21. 2026-08-22: 详情折叠块 marker 统一（用户反馈：「裁决的卡片里的🔻是什么鬼，统一一下」）：原生 `<details>` 的浏览器默认三角箭头（🔻）出现在 JudgementCard 与 HookJudgementPanel 两处的 `.field` 详情折叠块。修复：`summary` 隐藏默认 marker（`list-style:none` + `::-webkit-details-marker { display:none }`），summary 改 flex 行并前置统一 `.field-chevron`（与折叠行 `toggle-icon` 同一 chevron 词汇：14px 容器 / 12px svg / muted / `[open]` 时旋转 90°），两处 10 个 summary 全部对齐。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 22. 2026-08-22: 卡片高度减一档（用户反馈：「将卡片的高度减一档」）：CopyButton 由 26px 减至 22px（内嵌 icon 14px → 12px）。CopyButton 仅被 5 个消息区折叠块使用（NudgeBlock / ThinkingBlock / ToolCallBlock / ToolResultBlock / JudgementCard），作为折叠行高度基准，折叠行总高由 34px → 30px（22 + padding space-1×2），全卡同步一致。svelte-check 0 errors / 20 warnings（均为既有警告）。
- 23. 2026-08-22: 消息卡片设计规范沉淀（用户要求：「再补充一个消息卡片的设计规范，就参考现在的卡片设计总结一下」）：新增项目级规则 `.cursor/rules/ui-message-cards.mdc`（globs `**/*.svelte`，alwaysApply），汇总 5 个消息内联卡（JudgementCard / NudgeBlock / ThinkingBlock / ToolCallBlock / ToolResultBlock）已实现事实——容器、折叠行布局与 30px 高度、颜色层级（主体 text / 元信息 muted / 语义色仅小号状态文字）、图标（去 emoji，terminal/monitor 14px SVG）、展开详情、`details.field` marker 隐藏与 field-chevron、深色代码块、复制内容约定、整行可点交互模型。
- 24. 2026-08-22: 布局三区面板设计规范沉淀（用户要求为左侧/底部/右侧面板出设计规范，并以 FileExplorer 为简洁基准；经 AskUserQuestion 确认四项决策：全扁平列表 / 行高 24px / icon-btn 26×26 方形 / 状态徽标全局收敛为 fs-xs 语义色文字）。先调研三区全部面板现状（search 子代理输出 14 项不一致清单：根 padding 3 档、工具栏 8 形态、icon-btn 5 规格、徽标 5 套、本地 .btn 重定义 3 处、硬编码色值/圆角多处），再新增项目级规则 `.cursor/rules/ui-panel-layout.mdc`：通栏无根 padding、panel-toolbar 统一、24px 扁平行（hover 仅 tint、选中 primary 14% tint）、icon-btn 26×26、状态徽标克制型（计数徽标 mono tinted 例外）、分组标题 uppercase+0.04em 统一、禁硬编码 token 化、卡片式仅保留给聚合单元（裁决/课题/轮询）、错误提示条统一柔和型、hover-reveal 引用既有规则。
- 25. 2026-08-22: 三区面板执行收敛（用户批准「开始执行调整」）。11 个组件按 ui-panel-layout.mdc 收敛：**左** SessionList（激活 14% tint、mode-badge 去底改语义色文字、running 改 fs-xs 文字、行密度）、NeuronListPanel（type-badge 文字色、filter-bar 容器型、item 扁平化、row-btn 22px）、NeuronIndex（radius-sm、去 type-bar 竖条、caret 改 SVG chevron、选中 14%）、FileExplorer（icon-btn 26×26、行 24px、git-badge fs-xs）；**左/聚合** TopicPanel / PollerPanel / ToolPanel / GitPanel（错误条柔和型、GitPanel 行高 24px + 分组标题 uppercase + hover-reveal）；**底** LogPanel / TerminalPanel（硬编码色 → --color-error/--color-warning token）；**右** NeuronDetail（本地 .btn 删除走全局 btn-sm）、NeuronDetailDrawer（29 处硬编码圆角 token 化、头部 icon-btn 26×26、去 type-bar 竖条）。
- 26. 2026-08-22: 字号/字重对齐修正（用户两轮反馈：课题/工具/hook 字体对不上、工具 item 加粗）：**主文本字号统一 --fs-sm**（SessionList 标题 / NeuronIndex 行名 / TopicPanel 课题名 / NeuronListPanel item-desc / HookJudgement hook-badge 全部 11px → 13px；ToolPanel 工具名本为 fs-sm 不动）；**主文本字重统一常规不加粗**（去掉 TopicPanel 600、NeuronListPanel 600、SessionList 激活态 600、ToolPanel name/status/source 500），激活态仅靠 primary 14% tint 区分；hook 判定行高收敛 24px；错误提示条统一柔和型（error-bg 底 + error 字 + fs-sm）含 TopicPanel/HookJudgementPanel 两处残留实底红；清理 ToolPanel 残留 status-dot 空 span 与 GitPanel 4 条死 CSS。svelte-check 0 errors / 20 warnings（均为既有警告）。
