# Technical Plan / 技术方案: Hook 面板分页·命名·样式收敛

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-22_20-55_hook-panel-flow-decisions/requirements.md`
- 需求确认状态：已确认（三项决策 AskUserQuestion 拍板）
- 本方案覆盖范围：
  - `hook_judgements_list` 返回扩展 `{ records, total }`（Rust command + RPC 同步）
  - 前端面板滚动分页 + 过滤下沉后端 + 计数
  - 面板样式修复（单层滚动 + 行高收敛）
  - 展示名「流程决策 / Flow Decisions」（i18n key `views.flowDecisions`）

## Current Project Facts / 当前项目事实

- 后端已具备分页能力：`HookJudgementFilter { hook_type?, status?, conversation_id?, limit?, offset? }`（[store.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/core/hook/store.rs#L63-L70)）；`store.list` 已实现过滤 + `LIMIT/OFFSET` + `ORDER BY created_at DESC`，但**无总数**返回。
- `hook_judgements_list` command（[lib.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/lib.rs#L481)）返回 `Vec<HookJudgementRecord>`；RPC 分支同步（net/rpc.rs）。
- 前端面板 `HookJudgementPanel.svelte`：`refresh()` 全量拉取 `{ filters: {} }`，**前端内存过滤**（`filtered` derived），一次性渲染全部记录；事件驱动全量重拉。
- 样式问题根因：`.judgement-panel { overflow: auto }`（外层滚动）与 `.list { flex:1; min-height:0; overflow-y:auto }`（内层滚动）**双层滚动容器嵌套**，滚动条落在外层、toolbar/过滤条随内容滚动，「看不见行」。
- 命名现状：`views.hookJudgements`（zh「Hook 判定」/ en "Hook Judgements"）；视图 id `hook-judgements`；4 个 hook 标签 `hook.*`（范围完成/课题匹配/课题修订/评分反馈）。

## Open Questions / 开放问题

- 无。

## Solution Options / 方案候选

### 分页形态

- Option A：滚动自动加载（用户选定）——`.list` 滚动距底 < 阈值自动加载下一页；过滤切换重置第一页。
- Option B：加载更多按钮 / 经典页码——被用户否决（sidebar 空间有限）。

### 总数获取

- Option A（选定）：`store` 新增 `list_with_total`（单锁内 `COUNT(*)` + 分页 SELECT），command 返回 `{ records, total }`。
- Option B：前端只显示「已载入 M」不显示总数——计数信息弱，被否决。

### 展示名

- 「流程决策 / Flow Decisions」：i18n key `views.hookJudgements` → `views.flowDecisions`（值更新），视图 id `hook-judgements` 保留（layout 持久化安全），hook 标签 `hook.*` 不改。

## Decision / 方案决策

- Selected / 选定方案：
  1. 后端 `list_with_total`（单锁 COUNT + 分页 SELECT）→ command/RPC 返回 `{ records, total }`
  2. 前端滚动自动加载（PAGE_SIZE=50）+ 过滤下沉后端 + 计数显示过滤后总数 + 底部「已载入 M / 总数 N」+ 事件驱动刷新第一页
  3. 样式：`.judgement-panel` 改 `overflow: hidden`（滚动唯一归属 `.list`）；`.row` padding 收敛至 3px 上下（行高约 28px 基线）
  4. 命名：i18n `views.flowDecisions`，zh「流程决策」en "Flow Decisions"；引用点同步
- Decision Owner / 决策人：user（已确认）
- Decision Time / 决策时间：2026-08-22

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：扩展
  - `hook_judgements_list` 返回类型 `Vec<HookJudgementRecord>` → `HookJudgementListResult { records: Vec<HookJudgementRecord>, total: i64 }`（serde camelCase：`{ records, total }`）
  - 新增前端类型 `HookJudgementListResult`（types.ts）
- 消费方：前端「流程决策」面板；RPC 远程调用方
- 真相源文件：`core/hook/store.rs`、`lib.rs`（command）、`net/rpc.rs`、`src/lib/api/contracts.ts`、`src/lib/types.ts`

### 后端

```rust
// store.rs
pub struct HookJudgementListResult { pub records: Vec<HookJudgementRecord>, pub total: i64 }

impl HookJudgementStore {
    /// 单锁内先 COUNT（同过滤）再分页 SELECT，返回记录与总数。
    pub fn list_with_total(&self, filter: &HookJudgementFilter)
        -> AppResult<HookJudgementListResult>;
}
```

- `list_with_total` 复用 `list` 的 where 构造逻辑（抽 `build_where` 或内联两份 COUNT/SELECT），一次锁内完成，避免两次加锁。
- command（lib.rs）与 RPC（net/rpc.rs）返回 `HookJudgementListResult`。

### 前端

```ts
// types.ts
export type HookJudgementListResult = {
  records: HookJudgementRecord[];
  total: number;
};

// contracts.ts
hookJudgementsList: def<
  { filters?: HookJudgementFilter },
  HookJudgementListResult
>("hook_judgements_list");
```

### Compatibility Notes / 兼容说明

- 视图 id `hook-judgements` 不变（layout 持久化 JSON 不受影响）。
- i18n key 重命名 `views.hookJudgements` → `views.flowDecisions`：需同步 views.ts / HookJudgementPanel.svelte 引用点；hook 标签 `hook.*` key 与值不变。
- 后端返回结构变更会影响 RPC 远程调用方（如有）——本项目内仅前端面板消费，同步更新即可。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：本 technical-plan 经用户批准。
- 若执行前需求/API/范围变化：回写对应文档并重新确认。

### Step 1. 后端：list_with_total + command/RPC 返回扩展

- `core/hook/store.rs`：新增 `HookJudgementListResult` + `list_with_total`（单锁内 COUNT + SELECT，过滤与 `list` 一致）；保留 `list`（既有测试消费）。
- `lib.rs`：`hook_judgements_list` 返回 `HookJudgementListResult`。
- `net/rpc.rs`：`"hook_judgements_list"` 分支返回结构同步。
- 单测：`store.rs` 新增 `list_with_total` 断言（过滤后 total 正确 + 分页 records 正确）。
- 验收点：`cargo test --lib` 相关测试全绿。

### Step 2. 前端类型与契约

- `types.ts`：新增 `HookJudgementListResult`。
- `contracts.ts`：`hookJudgementsList` 返回类型改为 `HookJudgementListResult`。

### Step 3. 前端面板：滚动分页 + 过滤下沉 + 计数

- `HookJudgementPanel.svelte`：
  - 状态：`records` / `total` / `loading` / `loadingMore` / `hasMore`（`records.length < total`）
  - `PAGE_SIZE = 50`；`loadPage(reset: boolean)`：reset 时 `offset=0` 重拉第一页（total 更新），否则 `offset=records.length` 追加
  - 过滤变化（filterHookType/filterStatus）：重置第一页 + 列表滚动回顶（`.list.scrollTop = 0`）
  - 滚动加载：onMount 给 `.list` 绑定 `scroll` 监听（`scrollTop + clientHeight >= scrollHeight - 80` 且 hasMore 且 !loadingMore → loadPage(false)）；onDestroy 解绑
  - 事件驱动（hook_judgements）：`refresh()` 重置第一页（沿用既有实时语义）
  - 计数：`filter-bar .count` 显示 `total`（过滤后总数）；列表底部加载指示「已载入 M / 总数 N」+ loading 状态
  - `filtered` derived（前端过滤）删除，改由后端过滤结果直供
  - 空态：`total === 0` → 有过滤条件「无匹配记录」否则「暂无裁决记录」
- 验收点：滚动到底自动加载、过滤重置、计数正确、空态正确。

### Step 4. 样式修复 + 命名

- 样式：`.judgement-panel { overflow: hidden }`（移除 `auto`）；`.row { padding: 3px var(--space-2) }`（行高收敛至 ~28px）；`.list` 保持 `flex:1; min-height:0; overflow-y:auto`
- 命名：
  - `translations.ts`：`views.hookJudgements` → `views.flowDecisions`（类型 + en + zh，zh「流程决策」en "Flow Decisions"）
  - `views.ts`：`t("views.hookJudgements")` → `t("views.flowDecisions")`
  - `HookJudgementPanel.svelte`：面板标题引用同步
- 验收点：面板可见行正常滚动；标题「流程决策」；`pnpm check` 0 error。

### Step 5. 验证与回写

- 命令：`cargo check --lib`、`cargo test --lib`、`pnpm --filter pulsar-app check`、`pnpm --filter pulsar-app build`（如需要）。
- 回写 `lifecycle.md` 执行记录 + 验证结果。

## Risk And Mitigation / 风险与缓解

- 风险：滚动加载重复触发（滚动到底抖动）→ 缓解：`loadingMore` 锁 + 距底阈值 80px；加载完成前不重复请求。
- 风险：事件驱动刷新与滚动加载竞态 → 缓解：刷新为 reset 语义（重置 offset 与列表），滚动加载仅在非 loading 态触发；`refresh()` 时置 `loadingMore=false`。
- 风险：i18n key 重命名漏改引用 → 缓解：grep `views.hookJudgements` 全量替换后 `pnpm check` 兜底。
- 风险：`list_with_total` 与 `list` 过滤逻辑漂移 → 缓解：抽公共 where 构造（`build_where` helper），两者复用。

## Execute Checkpoint / 执行检查点

- 当前理解：面板分页（滚动自动加载）+ 过滤下沉后端返回总数 + 样式单层滚动收敛 + 展示名「流程决策」。
- 核心目标：裁决记录多时面板可用（分页）、可见（滚动修复）、语义准确（命名）。
- 下一步动作：等待用户批准本 technical-plan → 进入 executing（按 Step 1-5 执行）。
