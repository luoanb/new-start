# Requirements / 需求文档: workspace-file-management

## Restated Understanding / 需求复述

- 我理解当前需求是：为 pulsar-app 新增「文件管理」能力，分两个面向：
  - **面向 AI（nativetools）**：提供原生文件管理工具集，注册进现有 `ToolRegistry`（native 通道），供 Agent 会话调用。工具集对齐 Agent/IDE 文件操作能力清单（用户提供）：**Read（offset/limit 分段读大文件）、Write（覆盖已有文件前必须先 Read）、SearchReplace（SEARCH/REPLACE 语义替换首处匹配）、DeleteFile（一次多文件，须存在）、Glob（glob 模式查找）、Grep（ripgrep 语义：正则/大小写/多行/行号/上下文/类型过滤/计数）、LS（列目录，ignore 过滤）、GetDiagnostics（语言诊断 lint/编译错误）**。
  - **面向个人**：左侧 sidebar 新增「文件」视图（文件树，对标 VS Code 资源管理器）；在文件树上点击文件 → main 区新开编辑面板，主区域编辑文件内容，带语法高亮（CodeMirror 6）。
- 当前核心目标是：打通「可配置工作区」边界下的文件读写链路——AI 工具与 UI 共用同一套后端文件操作语义，均限定在已配置工作区根目录内。
- 当前边界是：作用域 = 可配置工作区（用户可指定/切换多个工作目录，存配置，每个工作区独立边界）；编辑器 = CodeMirror 6；UI = 左侧 sidebar 新增 tab + main 区编辑面板。
- 暂不处理：文件拖拽上传/多选批量操作、二进制文件可视化预览、diff 视图、Git 集成、超大文件虚拟化渲染、远程模式下的文件流式传输（先按现有 tauri command / rpc 普通请求模型实现）。GetDiagnostics 的实现深度（轻量 lint vs LSP）待技术方案阶段确认。

## Scope / 范围

- In:
  - 后端：工作区注册/列表/切换的存储与命令；文件系统操作命令（list/read/write/create_dir/delete/rename/move/glob/grep/info）；安全护栏（路径限定在已配置工作区根内、大小/数量限制、二进制检测、denylist）。
  - AI 原生工具（对齐用户提供的能力清单）：`list_directory`(LS) / `read_file`(Read，支持 offset/limit 按行分段) / `write_file`(Write，覆盖已存在文件前必须已 Read) / `search_replace`(SearchReplace，SEARCH/REPLACE 首处匹配) / `delete_file`(DeleteFile，一次多文件) / `glob`(Glob，glob 模式 + 修改时间排序) / `grep`(Grep，正则/大小写/多行/行号/上下文/类型过滤/计数) / `file_info` / `create_directory` / `rename` / `move`。注册为 native 工具，带 `inserts/<name>.md` 门禁。**GetDiagnostics v1 跳过**（不做语言诊断工具）。
  - 前端：左侧 sidebar 新增「文件」视图（文件树，懒加载目录、刷新、折叠/展开、默认过滤）；main 区新增「文件编辑器」面板类型（CodeMirror 6，语法高亮按扩展名/内容嗅探，保存回写，保存前检测外部修改）。编辑交互：右键菜单 + 快捷键（F2/Delete）+ 工具条新建；支持拖拽移动文件；删除直接删无确认；pad 端条目右侧 ⋮ 上下文按钮。
  - 事件：文件系统变更后刷新文件树（保存/删除/重命名后局部刷新）；新增 `workspaces` StateChange kind。
- Out:
  - 不做全文件系统范围（用户已选可配置工作区）。
  - 不引入 Monaco、不引入 LSP 语言服务器（CodeMirror 内置语法高亮即可；GetDiagnostics v1 跳过，后续迭代再议）。
  - 不做拖拽上传、多选批量、diff/Git、二进制预览、远程模式流式传输。
  - 不改动现有 chat / topic / neuron 等视图行为。

## User Interaction / 用户交互

- 触发入口：
  - 个人：左侧 sidebar「文件」tab → 工作区选择器（选择/添加/切换已配置工作区）→ 文件树浏览 → 点击文件在 main 区打开编辑面板。
  - AI：Agent 会话中模型自主调用文件工具（如 `read_file`、`write_file`、`search_replace`）。
- 用户操作路径：
  1. 首次：在「文件」视图选择工作区目录（本机文件夹选择器）→ 文件树加载根目录。
  2. 日常：展开目录 → 点击文件 → main 区打开编辑器 → 编辑 → 保存（Ctrl+S 或按钮）→ 文件树刷新。
  3. 工作区切换：视图内下拉/列表切换已配置工作区。
- 系统反馈：
  - 文件树节点：文件夹可展开/折叠，加载中态、错误态（无权限/不存在）明确提示。
  - 编辑器：顶部 tab 显示文件名与未保存标记（●）；保存成功/失败提示；语法高亮实时生效。
  - AI 工具执行结果以 JSON 返回（与现有 tool 返回格式一致），错误含可读 message + 工作区/路径上下文。
- 状态变化：
  - 工作区配置变更广播（`tools` 或新增 `workspaces` 事件 kind）→ 前端刷新。
  - 文件写操作（AI 或 UI）完成后文件树局部刷新。
- 异常/边界交互：
  - 打开的文件在工作区外（已被删除/移动）→ 编辑器提示文件缺失，禁用保存。
  - 保存时文件被外部修改 → 提示覆盖确认（可选，v1 可简化）。
  - 二进制/超大文件 → 拒绝在编辑器打开，提示原因。
  - 路径越界（如 `../` 逃逸工作区根）→ 后端拒绝，返回可读错误。
- 不应发生的交互：
  - UI 或 AI 能读写工作区根之外的文件。
  - 工具调用因路径规范化不一致而静默操作错误文件。
  - 文件树与编辑器状态长期失同步（保存后树仍显示旧内容/旧结构）。

## Acceptance Criteria / 验收标准

- [ ] 后端：可添加/列出/切换多个工作区根目录，配置持久化（存磁盘），重启后保留。
- [ ] 后端：文件系统命令在任意已配置工作区根下可用；路径越界/不存在/无权限返回可读错误，不 panic。
- [ ] AI 原生工具：上述 10+1 个文件工具全部注册进 ToolRegistry（native 来源，均有 `inserts/<name>.md`），`list_tools` 可见；工具执行走同一套工作区边界护栏。
- [ ] 前端：sidebar「文件」视图展示文件树（懒加载、刷新、折叠/展开、错误态）。
- [ ] 前端：点击文件在 main 区打开 CodeMirror 编辑器；支持语法高亮（常见语言至少 10 种）、保存（Ctrl+S/按钮）、未保存标记、保存后刷新树。
- [ ] 前后端联动：UI 保存的文件，AI `read_file` 能读到最新内容；AI `write_file` 后前端刷新可见。
- [ ] `cargo test --lib` 全绿；`pnpm check` 0 error；`vite build` 通过。

## Constraints / 约束

- 业务约束：
  - 遵循「可配置工作区」边界（用户决策）：任何读写不得越过已配置工作区根。
  - AI 工具行为语义对齐当前 IDE 文件工具（如 `search_replace` 首处匹配、`read_file` 支持 offset/limit 分段读大文件）。
- 技术约束：
  - Rust 后端：复用现有 `Tool` trait / `ToolRegistry` / `insert_catalog` 门禁；新增文件操作模块（可仿 `cmd_exec.rs` 的护栏风格）。
  - 前端：Svelte 5 + CodeMirror 6（新增依赖，记录版本）；视图注册走现有 `views.ts` / `layoutStore` 机制。
  - 大小限制：单文件读默认截断阈值（如 256KB，可分页 offset/limit 继续读）；写文件大小上限校验。
  - 不引入 LSP / 语言服务器。
- 时间/兼容性约束：
  - 不改动现有 tool 装配链契约（native 门禁保留）。
  - 前端现有视图（chat/topic/neuron 等）行为不变。

## Referenced Designs / 引用设计稿

> 本迭代无 Figma/视觉稿，交互形态来自迭代内与用户确认的交互设计。视觉事实展开见 `visual-design.md`。

| 用途 | Node | 链接 |
| ---- | ---- | ---- |
| 文件树/编辑器/右键菜单/拖拽/pad 交互设计 | —（交互确认，非 Figma） | `docs/sdd-lab/2026-08-16_23-45_workspace-file-management/visual-design.md` |

## Open Questions / 开放问题

- [x] Q1 工作区配置存放位置与数据结构？→ **独立 `workspaces.json`**（2026-08-16 已确认）
  - 与 mcp_servers.json / dynamic_tools.json 惯例一致，独立维护。
- [x] Q2 文件树懒加载深度与过滤规则？→ **懒加载 + 默认过滤**（2026-08-16 已确认）
  - 展开时按需加载子目录；默认隐藏 `.git` / `node_modules` 等（可配置 ignore 列表）。
- [x] Q3 编辑器保存冲突处理？→ **检测外部修改**（2026-08-16 已确认）
  - 保存前对比磁盘 mtime/size，外部有修改则提示确认。
- [x] Q4 远程模式（httpClient / SSE）下文件操作是否同接口？→ **同一 invoke 通道**（2026-08-16 已确认）
  - 文件命令走现有 `ApiClient.invoke`，远程自动经 RPC；事件新增 `workspaces` kind。
- [x] Q5 GetDiagnostics 实现深度？→ **v1 跳过诊断**（2026-08-16 已确认）
  - v1 不提供 get_diagnostics 工具、不做语言诊断，仅 CodeMirror 语法高亮；后续迭代再补。
- [x] Q6 Write/SearchReplace「覆盖已有文件前必须先 Read」落地？→ **内存清单校验**（2026-08-16 已确认）
  - 后端为工作区维护已读文件清单（路径+最后读取时间），write/search_replace/delete 覆盖前校验，未读则拒绝。

## Requirement Decisions / 需求决策

- 2026-08-16 23:45:
  - 决策：作用域 = 可配置工作区（多个工作目录，每个独立边界）；AI 工具集 = 完整读写（list/read/write/create_dir/delete/rename/move/search）；编辑器 = CodeMirror 6；UI = 左侧 sidebar 新增「文件」tab，点击文件在 main 区打开编辑面板。
  - 原因：用户在方案对齐中明确选择；可配置工作区兼顾安全与灵活性，CodeMirror 6 轻量可扩展，sidebar tab + main 编辑面板复用现有布局体系。
- 2026-08-16 23:50:
  - 决策：工作区列表存独立 `workspaces.json`；文件树懒加载 + 默认过滤（.git/node_modules 等可配置）；编辑器保存前检测外部修改（mtime/size 对比，有改动提示确认）；远程模式走同一 `ApiClient.invoke` 通道（RPC 自动转发），事件新增 `workspaces` kind。
  - 原因：四个开放问题逐一确认；对齐既有配置惯例、最小化远程改造、保存安全优先。
- 2026-08-16 23:55:
  - 决策：GetDiagnostics v1 跳过（不做语言诊断工具，仅语法高亮）；Write/SearchReplace「覆盖前必须先 Read」用内存清单校验（路径+最后读取时间，未读则拒绝）。
  - 原因：v1 控制范围、避免 LSP 复杂度；写前强校验落地用户对工具语义的要求。
- 2026-08-16 23:58:
  - 决策：技术方案选定 Option B——文件编辑器多实例 tab（按文件路径区分，兼容现有单实例面板语义）；工作区目录选择系统对话框 + 远程输入回退（引入 tauri-plugin-dialog）；过滤规则基于工作目录配置（每工作区独立 ignore，写入 workspaces.json，UI 可编辑）。
  - 原因：用户逐项确认；对齐 VS Code 体验（多标签 / 系统选目录 / 过滤可配置），布局多实例改造控制在 file-editor 类型。
- 2026-08-17 00:02:
  - 决策：编辑交互——右键菜单 + 快捷键（F2 重命名 / Delete 删除）+ 顶部工具条（新建文件/文件夹/刷新）；菜单按目标类型区分（空白/根、目录、文件）；新建/重命名 inline 输入框；删除直接删无确认；支持拖拽移动（命中 + 嵌套/循环 + 跨工作区校验，右键「移动」作备选）；pad 端条目右侧 ⋮ 上下文按钮。
  - 原因：用户逐项确认；对齐 VS Code 编辑习惯，拖拽提效，pad 适配保证无右键场景可用。
- 2026-08-17 00:10:
  - 决策：产出 `visual-design.md`（交互形态来自迭代内确认，非 Figma）；沿用现有 cool palette / ViewContainer tab / EditorTabs / 浮层词汇，不新增 token 体系；文件树选中态对齐 Neuron 列表；多实例编辑器 tab 对齐 EditorTabs 形态；icon 全部 inline SVG currentColor。
  - 原因：按 sdd-lab 规范将已确认交互落盘为设计文档，供技术方案执行引用。
