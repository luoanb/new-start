# 主面板 Tab 批量管理功能 —— 交互方案设计

> 状态：设计稿（待实现）
> 适用范围：主面板（`main` 区）Tab，即 `packages/pulsar-app/src/lib/layout/EditorTabs.svelte` 渲染的分栏面板标签页。
> 对应流程：主面板 `main.panes[].panels[]`（`LayoutStore.svelte.ts`）。

---

## 1. 背景与问题

当前主面板 Tab（`EditorTabs.svelte`）只支持**单人操作为主**的交互：

- 点击切换（`onSelect` → `layoutStore.setActivePanel`）
- 单 Tab 关闭（`onClose` → `+page.svelte#handleTabClose` → `layoutStore.closePanel`）
- 拖拽重排 / 跨分栏移动（Pointer Events + HTML5 DnD）

**缺失能力**：当分栏内累积大量面板（多个 chat 绑定窗口、文件编辑器、git-diff、神经元、工具编辑器等）时，用户无法批量关闭 / 批量保存 / 复制路径，只能逐个点 ✕，效率低。主流 IDE（VS Code、JetBrains/WebStorm、Sublime）均已提供"Tab 右键菜单 + 批量操作"范式。

---

## 2. 交互方案（总览）

参照主流 IDE，采用 **「Tab 右键菜单」+「批量选中态」** 双轨模式：

1. **单 Tab 右键**：弹出针对当前 Tab 的操作菜单（含批量入口）。
2. **批量选中态**：右键时若配合多选（Ctrl/Cmd + 点选、Shift + 点选范围），或右键菜单内选择"选择同类 / 全选"，可进入多选模式，随后的操作作用于整个选中集合。
3. **批量操作入口**：右键菜单呈现 5 类操作（见 §3），作用目标 = 右键点击的 Tab 或当前选中集合。

### 2.1 触发方式

| 入口 | 触发 | 说明 |
|------|------|------|
| 主右键 | 在任意 Tab 上 `contextmenu` | 弹出菜单，若该 Tab 已在选中集合内则保留集合，否则单 Tab 菜单 |
| 空栏空白区右键 | 在 tab 栏空白/spacer 上 `contextmenu` | 弹出"全部操作"菜单（作用于本分栏全部 Tab） |
| 键盘 | `Ctrl+K Ctrl+W`（关闭全部）/ `Ctrl+K Ctrl+W` 系列 | 快捷批量关闭（对齐 VS Code） |

> 注：`Editortabs` 现已在 `+page.svelte` 中渲染，右键需 `oncontextmenu` 拦截 `preventDefault()`。触屏设备（hover:none）提供 Tab 上的 ⋮ 菜单按钮作为等效入口。

---

## 3. 右键菜单项列表

菜单项在**原 Tab 所在分栏**（`pane`)上下文内作用。项目已有 `ContextMenu.svelte`（`GitPanel` / `FileExplorer` 已复用的通用组件）直接承载。

### 3.1 单 Tab 菜单（右键某 Tab）

| 菜单项 | 图标风格 | 动作 | IDE 参照 |
|--------|----------|------|----------|
| 关闭 | ✕ | `closePanel(tab.id)`（若有 dirty 走确认） | VS Code / WebStorm「Close」 |
| 关闭其他 | — | 关闭同分栏除当前外所有 Tab | VS Code「Close Others」 |
| 关闭右侧 | — | 关闭同分栏当前 Tab 右侧所有 Tab | VS Code「Close to the Right」 |
| 关闭全部 | — | 关闭同分栏全部 Tab | VS Code「Close All」 |
| 另存为… | 💾 | `file-editor` 触发后端另存（其余类型禁用灰化） | WebStorm「Save As…」 |
| 复制路径 | — | 复制工具提示里的完整路径（`file-editor` / `git-diff` / `commit-diff`） | VS Code「Copy Path」 |
| 复制相对路径 | — | 复制相对工作区路径 | VS Code「Copy Relative Path」 |
| 在新分栏打开 | ⧉ | `layoutStore.movePanelToNewPane(tab.id)` | VS Code「Move to New Group」 |
| ─ 分隔线 ─ | | |
| 选择同类 | ✓ | 批量选中同分栏内同 `type` 的 Tab（如全部 file-editor） | JetBrains 多 Tab 选择 |
| 全选 | ✓✓ | 批量选中同分栏全部 Tab | VS Code / WebStorm |

### 3.2 多选集合菜单（右键时已选中 ≥2 个 Tab）

在 3.1 基础上，动作作用于整个集合（`set`）：

| 菜单项 | 动作 |
|--------|------|
| 关闭选中的 N 个 | 逐个 `closePanel`（dirty 逐一确认 / 一次批量确认） |
| 关闭所选之外 | 关闭同分栏不在集合中的 Tab |
| 另存选中的 | 对集合内每个 `file-editor` 触发保存 |
| 复制选中路径 | 一次复制集合内全部路径（换行分隔，经剪贴板） |
| 取消选择 | 清空集合，退出多选态 |

### 3.3 空栏空白区菜单（tab 栏 spacer 右键）

| 菜单项 | 动作 |
|--------|------|
| 全部关闭 | 关闭本分栏全部 Tab |
| 全部关闭并保存 | 先保存全部 dirty 的 `file-editor`，再关闭全部 |
| 保存全部 | 对所有 dirty 的 `file-editor` 触发保存 |
| 全选 | 选中本分栏全部 Tab（进入多选态） |

---

## 4. 批量选择方式

采用主流 IDE 的多选模型，作用于**单个分栏（pane）内**：

- **单击选中**：`Ctrl/Cmd + 点击` 增删选中；普通点击恢复为单选切换（维持现有 `onSelect` 语义）。
- **范围选中**：`Shift + 点击` 选中从"上次锚点"到当前 Tab 的连续区间。
- **批量入口**：右键菜单「全选」「选择同类」。
- **状态存储**：选中集合为组件级 `$state`，随 `+page.svelte` 的 `paneTabs`/`layoutStore` 变化做内存去重（Tab 关闭即从集合剔除）。

选中态视觉：Tab 高亮为 `--color-primary` 淡底 + 细边框，与激活态区分（激活态保留顶部色条）。

### 4.1 选中集合与状态同步

选中集合 `selectedIds: Set<string>` 仅针对当前分栏。集合在以下场景自动收敛：

- 有 Tab 被 `closePanel` 关闭 → 从集合删除。
- 分栏被整体移除（空）→ 清空集合。
- 切换到其他分栏的 Tab → 现有实现每分栏各自渲染 `EditorTabs`，选中态天然按分栏隔离。

---

## 5. 操作入口（调用链）

所有批量操作最终都汇聚到 `LayoutStore`（唯一的状态真源）+ 少量 `+page.svelte` 编排（dirty 确认 / 文件保存 / 剪贴板）。

```
EditorTabs.svelte (右键菜单 + 多选态)
   │  onBatchClose / onClose / onSave / onCopyPath / onMoveNewPane
   ▼
+page.svelte (handleTabClose / handleBatchClose / handleSave / copy)
   │  layoutStore.closePanel / movePanel / movePanelToNewPane
   │  fileEditorStore.isDirty / dispose / save
   ▼
LayoutStore.svelte.ts (state.main.panes 状态)
```

### 5.1 `EditorTabs.svelte` 新增 Props

在现有 `onSelect/onClose/onDrop/onDropToNewPane` 基础上扩展：

```ts
tabs: ViewMeta[];              // 现有
activeId: string | null;
paneId: string;
onSelect, onClose, onDrop, onDropToNewPane;   // 现有
onBatchClose: (ids: string[]) => void;        // 新增：批量关闭
onCloseOthers: (id: string) => void;          // 新增（基于 closePanel 编排）
onCopyPath: (id: string) => void;             // 新增：复制路径
onSave: (id?: string) => void;                // 新增：保存（可作用于选中）
pinned: (id) => boolean;                      // 新增：判断是否不可关（如主 chat 面板）
```

### 5.2 `LayoutStore` 补充能力

`closePanel` 已可单关；批量关闭 = 对集合循环 `closePanel`（其内部已处理空分栏收缩与焦点迁移）。新增便捷方法`closePanelMany(ids: string[])` 可选封装，避免 `+page` 重复编排。

---

## 6. 边界情况处理

| # | 边界情况 | 规则 | IDE 参照 |
|---|----------|------|----------|
| B1 | **主 chat 面板不可关** | 全局主 chat（`type==="chat"` 且非 `chat:` 绑定窗口，即主窗口）为固定 Tab，右键「关闭」置灰禁用，且从所有批量关闭中排除。绑定窗口（`chat:${conversationId}`）可关。 | WebStorm「Detached / pinned editor」 |
| B2 | **未保存（dirty）Tab 批量关闭** | 若集合含 dirty 的 `file-editor`：弹一次批量确认对话框（列出 dirty 文件数 + 列举），确认后丢弃关闭；取消则不关闭任何 dirty Tab（干净 Tab 直接关）。 | VS Code / WebStorm 未保存关闭确认 |
| B3 | **批量关闭后焦点迁移** | `closePanel` 已内建：分栏仍有余面板则激活相邻（`activePanelId = panels[min(idx, len-1)]`）；空分栏移除并激活相邻分栏。批量关闭后确保选中集合清空、`activePanelId` 有效，缺失时回退首个面板。 | VS Code 关闭尾 Tab 激活相邻 |
| B4 | **末尾 Tab 关闭** | 关闭的是分栏最后一个 Tab：分栏置空 → 移除该分栏 → 回退到相邻分栏（与单关一致）。批量关闭"全部"等同清空分栏。 | — |
| B5 | **不可应用的操作灰化** | 「另存为 / 保存」仅对 `file-editor` 有效，其余类型灰化置灰；「复制路径」仅对有路径的 `file-editor`/`git-diff`/`commit-diff` 可用；chat/神经元等无路径类型禁用。 | VS Code 菜单上下文感知 |
| B6 | **单个分栏 vs 全部分栏** | 批量语义默认限定**当前分栏**；如需全局「关闭全部分栏」需单独入口（空格栏菜单「全部关闭」当前分栏；跨栏统一关闭不在本次范围，避免误伤）。 | VS Code「Close All Groups」独立项 |
| B7 | **dirty 与「另存」冲突** | 对 dirty Tab 执行「另存为」须先触发保存流程，成功后置脏标记清除（依赖 `fileEditorStore` 状态响应式更新 `dirty`）。 | — |
| B8 | **复制路径的平台剪贴板** | 经 Tauri clipboard 插件写入；空选中/无路径时静默失败或 Toast 提示，不抛错。 | — |

### 6.1 焦点迁移保证（B3 细化）

`LayoutStore.closePanel` 现有逻辑已内建正确迁移。批量关闭需保证**执行顺序与迁移一致**：建议从集合末尾向头逐个关闭（索引靠后的先关，减少相邻激活索引漂移），或先统一收集"仍存活"面板再做一次 `activePanelId` 收敛。实现时统一走 `+page.svelte` 编排的 `handleBatchClose`。

---

## 7. 状态同步策略

1. **`dirty`（未保存 ●）**：来源 `fileEditorStore.isDirty(panelId)`，`paneTabs()` 每派生读取，关闭/保存后自动刷新 —— 批量关闭确认时直接读取该 store 判断。
2. **可关闭性（pinned）**：`pinned(id)` 判定 `type === "chat" && !id.startsWith("chat:")`（全局主 chat 固定）。若未来引入文件固定，扩展为已打开文件集合 + 固定标记。
3. **选中集合**：`EditorTabs` 内部 `$state`，与 `layoutStore` 无持久化耦合（避免污染布局存储 schema v11）。

---

## 8. i18n

菜单文案统一走 `$lib/i18n`，新增命名空间 `editorTabs.`：

```
editorTabs.close            = 关闭
editorTabs.closeOthers      = 关闭其他
editorTabs.closeRight       = 关闭右侧
editorTabs.closeAll         = 关闭全部
editorTabs.saveAs           = 另存为…
editorTabs.save             = 保存
editorTabs.saveAll          = 保存全部
editorTabs.copyPath         = 复制路径
editorTabs.copyRelativePath = 复制相对路径
editorTabs.moveNewPane      = 在新分栏打开
editorTabs.selectSameType   = 选择同类
editorTabs.selectAll        = 全选
editorTabs.deselect         = 取消选择
editorTabs.closeSelected    = 关闭选中的 {n} 个
editorTabs.closeAllAndSave  = 全部关闭并保存
editorTabs.unsavedDialogTitle = 关闭未保存的标签页？
editorTabs.unsavedDialogBody  = 有 {n} 个标签页未保存，关闭将丢失更改。是否继续？
```

---

## 9. 主流 IDE 参照对照表

| 交互 | VS Code | JetBrains (WebStorm) | 本项目落地 |
|------|---------|----------------------|-----------|
| Tab 右键菜单 | ✔ | ✔ | ✔（ContextMenu 复用） |
| Close / Close Others | ✔ | ✔ Close Others | ✔ |
| Close to the Right | ✔ | — | ✔ |
| Close All | ✔ | ✔ | ✔ |
| Split / Move to New Group | ✔ | ✔ | ✔（复用 movePanelToNewPane） |
| Copy Path / Copy Relative Path | ✔ | ✔ | ✔ |
| 多选 Tab（Ctrl+点选 / Shift 范围） | 受限 | ✔（多选 editor tab） | ✔ |
| 未保存批量关闭确认 | ✔ | ✔ | ✔（B2） |
| Pinned / 不可关 Tab | 固定标签 | Detached | ✔（B1 主 chat） |

---

## 10. 实施拆解（后续待办对应）

1. **实现右键菜单 + 批量关闭**：扩展 `EditorTabs` 加 `contextmenu` 与 `ContextMenu` 挂载；`+page.svelte` 新增 `handleBatchClose`；`closePanelMany`（可选）。
2. **实现批量保存 / 复制路径 / 另存**：`onSave/onCopyPath` 对接 `fileEditorStore` 与 Tauri clipboard；复制路径用 Tab 的 `tooltip` 字段。
3. **边界处理与状态同步**：pinned 判定、dirty 批量确认（复用 `ConfirmDialog`）、焦点迁移验证、灰化规则。
