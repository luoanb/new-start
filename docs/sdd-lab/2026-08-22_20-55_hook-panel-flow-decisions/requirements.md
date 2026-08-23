# Requirements / 需求文档: Hook 面板分页·命名·样式收敛

## Restated Understanding / 需求复述

- 我理解当前需求是：对已交付的裁决记录面板（`hook-judgements` 视图）做三项收敛——
  1. **分页**：裁决记录全量保留且只增不删，目前面板一次性全量拉取渲染，需要分页；
  2. **样式**：面板当前「看不见行」（列表无独立滚动，滚动区塌陷），且卡片行高偏大；
  3. **命名**：「Hook 判定」这个名字带英文黑话且语义窄，需要更好的展示名。
- 当前核心目标是：面板按**滚动自动加载**分页、过滤下沉后端、计数显示总数；修复滚动容器嵌套 bug 并收敛行高；展示名改为**「流程决策 / Flow Decisions」**（对齐 hook-inject-points 迭代确立的注入点调度语义，为后续收纳非裁决 hook 留空间）。
- 当前边界是：只改**用户可见展示名**与面板交互/样式；不改底层技术概念名（`HookDef` / `hook_judgements` 表 / 视图 id `hook-judgements`）；不改 4 个 hook 标签（范围完成 / 课题匹配 / 课题修订 / 评分反馈）。

## Scope / 范围

- In:
  - 面板分页：滚动到底自动加载下一页；过滤（hook_type / status）下沉后端 `HookJudgementFilter`；计数显示「总数 / 已载入」
  - `hook_judgements_list` 返回扩展：`{ records, total }`（总数供计数与 hasMore 判断）
  - 样式修复：外层容器改单层滚动（`.list` 独立滚动），卡片行高收敛（对齐 visual-design 28px 行高基线）
  - 展示名：面板标题 zh「流程决策」en "Flow Decisions"；i18n key `views.hookJudgements` 重命名为 `views.flowDecisions`（值更新，视图 id 保留）
- Out:
  - 视图 id / 底层表名 / `HookDef` 等技术概念改名
  - 4 个 hook 标签改名（保持「范围完成/课题匹配/课题修订/评分反馈」）
  - 消息内联裁决卡（JudgementCard）改动（分页/命名均不涉及；若面板命名影响卡文案，仅同步 i18n 值）
  - 后端数据迁移（表结构不变）

## User Interaction / 用户交互

- 触发入口：sidebar「流程决策」视图（原「Hook 判定」）
- 用户操作路径：
  - 打开面板 → 时间线列表（created_at 倒序）→ 滚动到底自动加载更多 → 切换过滤下拉即时重查（重置到第一页）
  - 计数：过滤条显示「总数 N」，列表加载时底部显示「已载入 M / 总数 N」（或等价信息）
  - 点击记录展开详情（payload / attempts_detail / raw / decision / error）→ 「在会话中定位」不变
- 系统反馈：
  - 滚动到底时短暂 loading（底部指示），加载完成追加列表
  - 新裁决事件到达时刷新第一页（保持既有实时行为）
- 状态变化：无（纯展示层）
- 异常/边界交互：
  - 过滤后无记录 → 空态「无匹配记录」；全部无记录 → 空态「暂无裁决记录」（沿用既有）
  - 过滤切换时列表重置并滚动回顶部
- 不应发生的交互：
  - 外层容器与列表双层滚动（列表必须独立滚动）
  - 分页后过滤仅在已加载子集内生效（必须后端过滤）

## Acceptance Criteria / 验收标准

- [ ] `hook_judgements_list` 返回 `{ records, total }`；`total` 为过滤后总数；RPC 与 Tauri command 同步
- [ ] 面板滚动到底自动加载下一页；过滤切换重置到第一页并从后端重查
- [ ] 过滤条计数显示过滤后总数；列表加载中/加载完显示「已载入 M / 总数 N」（或等价）
- [ ] 面板滚动修复：`.list` 独立滚动，外层容器不滚动；可见记录行（不再「看不见行」）
- [ ] 卡片行高收敛至 28px 基线（或更紧凑），全量渲染时一屏信息量合理
- [ ] 展示名：面板标题 zh「流程决策」en "Flow Decisions"；i18n key `views.flowDecisions`；视图 id 仍为 `hook-judgements`
- [ ] `cargo test --lib` 全绿（含新增 total/分页单测）；`pnpm --filter pulsar-app check` 0 error；面板人工验证可用

## Constraints / 约束

- 技术约束：
  - 过滤语义必须后端生效（`HookJudgementFilter { hookType, status, limit, offset }` 已支持，需扩展 total）
  - 视图 id `hook-judgements` 不变（涉及 layout 持久化）；i18n key 可重命名
  - 不引入新外部依赖
- 时间/兼容性约束：
  - 事件驱动刷新沿用「全量重拉第一页」语义（裁决记录量级小，重拉成本可忽略）

## Open Questions / 开放问题

- 无（三项决策已确认）。

## Requirement Decisions / 需求决策

- 2026-08-22:
  - 决策：展示名改为「流程决策 / Flow Decisions」，i18n key `views.hookJudgements` → `views.flowDecisions`；视图 id `hook-judgements` 保留
  - 原因：hook 概念已泛化为注入点机制（IP-1~5），「Hook 判定」语义窄且带英文黑话；「流程决策」对齐新架构语义并预留收纳空间
  - 决策：分页用滚动自动加载；过滤下沉后端并返回总数
  - 原因：sidebar 面板空间有限，滚动加载最自然；分页后前端无法本地过滤，必须后端过滤 + total 支撑计数与 hasMore
  - 决策：样式修外层双层滚动嵌套 + 卡片行高收敛
  - 原因：用户反馈「看不见行了」= 滚动区塌陷（`.judgement-panel` overflow:auto 与 `.list` overflow-y:auto 嵌套），非行高过大；行高收敛为附带优化
