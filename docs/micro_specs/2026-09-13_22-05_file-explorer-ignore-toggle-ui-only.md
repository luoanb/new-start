# Spec: 文件树「忽略过滤」开关（仅影响用户侧 UI 展示）

来源：用户需求「新增一个文件过滤开关，默认关闭；仅当开启时，用户页面才过滤掉忽略的目录和文件」（2026-09-13）。
方案确认：用户在 A / B / C 三案中明确选择 **C —— 开关仅影响用户侧的 UI 展示**。

## Goal

- 要解决什么问题：默认情况下文件树把工作区 ignore 命中项（`.git` / `node_modules` / `target` …）隐藏，用户看不到、也无法在树里直接操作它们；需要给用户一个**自主开关**。
- 验收结果：
  1. 开关位于「编辑过滤规则」弹窗（`fileExplorer.ignoreTitle` 那个 modal）内，文案为「在文件树中应用过滤」/ "Apply filtering in the file tree"，**默认关闭**；
  2. 关闭时（默认）：文件树展示**不过滤**的所有条目（含 ignore 命中项）；
  3. 开启时：文件树隐藏 ignore 规则命中的目录与文件（= 改动前的行为）；
  4. 开关状态跨重启保留；
  5. **AI 侧行为零变化**：AI 的 `ls`/`glob`/`grep`、语义索引、git 仓库发现仍一律遵守工作区 ignore 规则。

## Done Contract

- 什么算完成：开关落在文件树工具条，切换后已加载目录立即重载（保留展开态）；偏好持久化；`fs_list` 是唯一受影响的通道。
- 由什么证明：`pnpm --filter pulsar-app check` 0 error；App 内切换开关观察树内容变化；重启 App 后开关状态保持；AI 工具在开关开启/关闭下对 `node_modules` 的可见性一致。
- 哪些情况仍算未完成：AI 侧可见性随该开关变化（越界，等于把 C 做成了 A）。

## Scope

- In：
  - `src/lib/components/FileExplorer.svelte`：开关 UI（置于「编辑过滤规则」弹窗内，`ignoreEdit` modal）+ 偏好持久化 + `fs_list` 参数选择 + 切换后重载；
  - `src/lib/api/contracts.ts`：`fsList` 契约补 `ignore?: string[]`；
  - `src/lib/i18n/translations.ts`：`fileExplorer` 段新增 2 个键（类型 + en + zh）。
- Out：
  - **Rust 侧零改动**：`fs_list` 命令与 `fileops::fs::list` 早已支持可选 `ignore`（`fs.rs:247` `ignore.unwrap_or(&workspace.ignore)`），传空数组即"不过滤"，无需新增命令/配置节/运行时开关（**仅**新增 1 条回归测试 `list_with_empty_ignore_shows_ignored_entries` 钉住"空规则集 = 不过滤"语义，无行为改动）；
  - 不新增 `config.json` 配置节（UI 展示偏好，沿用仓库既有 `localStorage` 偏好先例：`theme.ts`、`neuron/networkLayout.ts`、`NeuronDetailDrawer.svelte`）；
  - AI 工具（`fs_tools.rs` 的 ls/glob/grep）、语义索引（`search/retriever.rs`）、git 仓库发现（`gitops/repo.rs`）一律不动；
  - 不改工作区 ignore 规则本身（已有「编辑过滤规则」入口）。

## Facts / Constraints

- ⚠️ **与 🔒 盖章章节的已知冲突（用户已裁决）**：`docs/pulsar/fileops/index.md` §8.2「AI 工具与前端 UI 共用同一工作区集合与护栏，不允许任何一方有特权旁路」、§8.3「ignore 规则在所有枚举/检索路径上一致生效」，以及 §3 表格「过滤」行「列目录 / glob / grep / 索引全部遵守工作区 ignore 规则」。方案 C 使 **UI 列目录**不再恒守 ignore，构成本条自认的分歧。
  - **用户裁决**：选择 C（2026-09-13），据此允许调整该章节所约束的代码。
  - **本轮未改该文档**：修正 §3 / §8.2 / §8.3 措辞需用户明确「解章」，Agent 不自行解章或改章。
- 用户可见性口径：忽略规则仍**存在且默认生效于 AI 侧**；C 只是让「用户在树里看什么」可调。
- `fs_list` 两种调用语义（`fs.rs:247`）：
  - `ignore` 缺省 → 用 `workspace.ignore` → **过滤**；
  - `ignore: []` → 规则集为空 → `is_ignored` 恒 false → **不过滤**（符号链接仍恒不展示，与开关无关）。
- `fs_list` 是用户侧树与「移动目标目录选择器」的唯一列表通道（`FileExplorer.svelte` 两处调用）；AI 侧不经过它。

## Restated Understanding

- 我理解当前任务是：让用户能自己决定文件树是否隐藏被忽略的目录/文件，默认不隐藏。
- 当前核心目标是：开关只作用在用户侧的展示通道上，别的一律不碰。
- 当前边界是：只动 3 个前端文件；零 Rust 改动；不碰盖章文档。

## 接口契约设计

```ts
// contracts.ts —— 复用后端已存在的可选规则集参数（空数组 = 不过滤）
fsList: def<{ path?: string; ignore?: string[] }, FsEntry[]>("fs_list");

// FileExplorer.svelte
const APPLY_IGNORE_KEY = "pulsar.fileExplorer.applyIgnore";   // "1" = 开启过滤；缺省/false = 关闭
let applyIgnore = $state(readApplyIgnore());                  // 默认 false
function listArgs(path: string): { path?: string; ignore?: string[] };
function setApplyIgnore(next: boolean): void;                 // 持久化 + 重载已加载目录
```

## Checkpoint Summary

- 当前任务理解：文件树忽略过滤开关，默认关，仅 UI 生效。
- 当前核心目标：一个开关 + 一条参数选择分支，零后端改动。
- 当前进度：spec + 代码完成，静态检查通过，待 App 内人工确认。
- 涉及文件 / 模块：`FileExplorer.svelte`、`api/contracts.ts`、`i18n/translations.ts`。
- 风险：关闭时树里可点开 ignore 命中项（如 `node_modules`），大目录懒加载仍按需触发，不会一次性读入（无上下文风险）；仅展示层差异，读写护栏不变。
- 验证方式：`pnpm --filter pulsar-app check`；App 内切换 + 重启；AI 侧对照。
- Execution Approval: 用户选择方案 C + 需求原文（2026-09-13）。

## Change Log

- 2026-09-13：初始记录。新增文件树「过滤忽略项」开关（默认关），`fs_list` 关时传空规则集、开时省略参数。
- 2026-09-13（修订，用户反馈驱动）：① **位置**由树工具条移至「编辑过滤规则」弹窗（`ignoreEdit` modal）内，与规则文本同屏，避免误以为是视图工具栏的功能；② **命名**由「过滤忽略项」改为「仅用户视图过滤」/“Filter in user view only”，直接点明作用域仅限用户视图；tooltip 与弹窗内说明同步写明「AI 视角、搜索索引与 git 仓库发现不受影响」；③ 随位置调整样式类 `ignore-toggle` → `ignore-scope`（模态内块级布局，`margin-top: var(--space-3)`）。
- 2026-09-13（文案修订 2，用户反馈「命名不准、表述不清」）：标签由「过滤忽略项」改为「仅在用户视图中过滤」；说明文案去掉欧化被动「被记住」与行话，并统一使用界面既有术语「过滤规则」（此前混用了「忽略规则」）。
- 2026-09-13（文案修订 3，用户要求「仔细品味开关含义后重组文案」）：
  - **语义澄清**：本开关只控制「文件树要不要套用过滤规则」；AI 侧（文件工具、搜索索引、git 仓库发现）**不受开关影响，始终套用规则**。因此开关的关闭态只能读作「文件树不筛选、显示全部」，**不能**读作「过滤也作用于 AI」。
  - **改写理由**：前一版标签「仅在用户视图中过滤」是**范围声明**而非可勾选的状态声明——取消勾选时字面会误导为「过滤不再仅限用户视图（即波及 AI）」。改为对文件树的**状态陈述**「在文件树中应用过滤」（勾选=隐藏命中项；不勾选=显示全部），范围限制（仅界面）移入说明文字。
  - 键名同步：`userViewFilter` → `applyFilterInTree`；组件侧去掉重复的 tooltip 标题（说明文字已在下方，避免同一句话出现两次）。
- 静态检查复跑：`pnpm --filter pulsar-app check` → 0 errors / 20 warnings（与修订前同数，未新增）。
- 2026-09-13（收尾证据）：① `pnpm --filter pulsar-app run build` 通过（`built in 10.75s`，`build/` 已产出）——覆盖用户实际运行的生产产物；② 新增 Rust 回归测试 `list_with_empty_ignore_shows_ignored_entries`（在 `fileops/fs.rs` 的测试模块内，只有测试、无行为改动），用于固定前端所依赖的「关闭时传空规则集 ⇒ 不过滤」这一语义；`cargo test --lib` → `503 passed; 0 failed`。

## Validation

- Self-check: 唯一受影响的通道是 `fs_list`（用户侧）；传空数组即绕过过滤，`is_ignored` 不可能命中。
- Static checks: `pnpm --filter pulsar-app check` → `svelte-check found 0 errors and 20 warnings in 7 files`（rc=0）。20 条告警均为既有告警：`ModelPicker.svelte`×12、`ViewHost.svelte`×2、`ProvidersModelsPanel.svelte`×2、`Tooltip.svelte`/`SuggestInput.svelte`/`PathInput.svelte`/`FileExplorer.svelte` 各 1；其中 FileExplorer 的 1 条为 `:859` 既有的 `a11y_no_static_element_interactions`（在本次改动行 117-190 / 690 / 774-781 / 1044-1057 之外），**本次改动未新增告警**。
- Runtime / Test: `cargo test --lib fileops::fs::` → `12 passed; 0 failed`（含新增 `list_with_empty_ignore_shows_ignored_entries`）；`cargo test --lib` → 全量通过。
- 生产构建: `pnpm --filter pulsar-app run build` → `✓ built in 10.75s` + `Wrote site to "build"`（rc=0）；同时确认 `readApplyIgnore()` 的 `localStorage` 守卫在 `adapter-static` 预渲染阶段不会因 `localStorage` 不存在而报错。
- Human confirmation: 待用户 App 内确认（切换开关树内容变化；重启保留；AI 侧不变）。
- 结果汇总：静态检查通过；人工确认待补。
- 核心目标是否已由证据证明完成：否（差人工确认）。
- 剩余风险：盖章文档 §3/§8.2/§8.3 与实现的分歧**尚未在文档层消解**（等用户解章）。

## Resume / Handoff

- 当前状态：代码完成，待验证收尾。
- 当前卡点：文档层冲突需用户「解章」后方可同步。
- 下一步唯一动作：App 内切换开关确认树内容变化 + 重启确认持久化。
- 下一轮核心目标：用户解章后，把 §3「过滤」行与 §8.2/§8.3 修订为「默认一致 + 用户可显式放宽 UI 展示」的口径。
