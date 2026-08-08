# Technical Plan / 技术方案: tool-panel-ui-polish

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-08-08_18-13_tool-panel-ui-polish/requirements.md`
- 需求确认状态：已确认（Q1–Q2，2026-08-08 18:13）
- 本方案覆盖范围：ToolPanel 全量 i18n + transport/method 换 `Select` + disabled 改 toggle

## Current Project Facts / 当前项目事实

- `lib/components/ToolPanel.svelte`：列表区与编辑弹窗文案全部硬编码（中英混合）；transport（L295）/ method（L352）用原生 `<select>`；disabled 为 checkbox（L300，`.field-toggle` 可收缩导致标签被裁切）。
- `lib/components/Select.svelte`：现有封装组件，`value`（bindable）+ `options: {value,label}[]` + `onchange` + 键盘导航 + portal 浮层。
- `lib/i18n/`：`index.svelte.ts` 导出 `t(key, params?)` / `tMap(prefix, subKey)`；`translations.ts` 定义 `Translations` 类型 + `en` / `zh`（类型强制两语言键完整）。
- 无现成 toggle/switch 组件（仅 checkbox 与 segmented `.mode-toggle`）。
- 其他面板（sidePanel / neuronPanel 等）已接入 i18n，可作为字典组织参考。

## Solution Options / 方案候选

| 决策点 | 候选 | 选定 | 原因 |
|---|---|---|---|
| select | 原生（现状）/ 封装 `Select` | **封装 `Select`** | 与全项目控件一致，键盘 + portal + 视觉统一 |
| disabled 开关 | checkbox（现状）/ toggle 新组件 | **toggle 新组件 `Toggle.svelte`** | 语义清晰；`role="switch"` 可访问；复用性 |
| i18n 范围 | 仅弹窗 / 整个 ToolPanel | **整个 ToolPanel** | 列表区同样硬编码；用户确认统一改 |
| 技术标识符 | 翻译 / 保留 | **保留**（native/config/mcp、stdio/http 不译） | 标识符性质，翻译反而增加认知负担 |

## Decision / 方案决策

- Selected：ToolPanel 全量接入 i18n（新增 `toolPanel` 命名空间）；transport/method 换 `Select.svelte`；disabled 改 `Toggle.svelte`。
- Why：修复用户反馈的控件不一致、标签裁切、多语言缺失三问题；复用现有 Select 与 i18n 系统。
- Decision Owner：用户（已确认）
- Decision Time：2026-08-08 18:13

## Open Questions / 开放问题

- 无（Q1–Q2 已在需求文档确认）。

## API Design / API 设计

### Contract Scope

- 变更类型：修改（ToolPanel）+ 扩展（i18n 字典、新组件 Toggle）。
- 消费方：ToolPanel；i18n 字典被全局消费（类型强制）。
- 真相源文件：`src/lib/components/ToolPanel.svelte`、`src/lib/i18n/translations.ts`、新增 `src/lib/components/Toggle.svelte`。

### i18n 字典 `toolPanel` 命名空间（translations.ts 三处同步：类型 + en + zh）

```ts
toolPanel: {
  // 列表区
  title: string;            // 工具 / Tools
  refresh: string;          // 刷新 / Refresh（aria-label）
  editConfig: string;       // 编辑配置 / Edit config
  loading: string;          // 加载中… / Loading…
  mcpSection: string;       // MCP Servers
  toolsSection: string;     // 工具 / Tools
  toolsCount: string;       // {count} tools / {count} 个工具
  noMcpServers: string;     // 暂无 MCP server，点右上角「编辑配置」添加 / No MCP servers…
  noTools: string;          // 暂无可用工具 / No tools available
  status: Record<string, string>;  // connecting/connected/failed/disabled → 连接中/已连接/失败/已停用
  // 弹窗
  modalTitle: string;       // 工具配置 / Tool configuration
  modalAria: string;        // 工具配置编辑 / Edit tool configuration（aria-label）
  close: string;            // 关闭 / Close
  loadingConfig: string;    // 加载配置中… / Loading config…
  add: string;              // 添加 / Add
  delete: string;           // 删除 / Delete
  emptyMcp: string;         // 暂无 MCP server / No MCP servers
  emptyHttp: string;        // 暂无 HTTP tool / No HTTP tools
  emptyCommand: string;     // 暂无 command tool / No command tools
  name: string;             // 名称 / Name
  transport: string;        // 传输方式 / Transport
  method: string;           // 方法 / Method
  command: string;          // 命令 / Command
  args: string;             // 参数（逗号分隔）/ Args (comma separated)
  url: string;              // 地址 / URL
  timeoutMs: string;        // 超时（毫秒）/ Timeout (ms)
  desc: string;             // 描述 / Description
  template: string;         // 模板（命令模板）/ Template (command)
  disabled: string;         // 停用 / Disabled
  transportHint: string;    // stdio 需 command；http 需 url / stdio requires command; http requires URL
  httpUrlHint: string;      // 端点固定，{'{query}'} 由模型填充 / Fixed endpoint; {'{query}'} filled by the model
  commandHint: string;      // 命令经过安全护栏：denylist / 超时 / 并发 / Command passes safety rails: denylist / timeout / concurrency
  saveHint: string;         // 保存即生效：写回 JSON 并触发全量重装配 / Saved immediately: writes JSON and reassembles
  cancel: string;           // 取消 / Cancel
  save: string;             // 保存 / Save
  saving: string;           // 保存中… / Saving…
  loadFailed: string;       // 加载配置失败 / Failed to load config
  reassembleFailed: string; // 重新装配失败 / Reassemble failed
  loadListFailed: string;   // 加载失败 / Failed to load
}
```

### 新增 `Toggle.svelte`

```svelte
<script lang="ts">
  let { checked = $bindable(), label = "", disabled = false }: {
    checked?: boolean;
    label?: string;
    disabled?: boolean;
  } = $props();
</script>

<label class="toggle">
  <input
    type="checkbox"
    class="toggle-input"
    role="switch"
    aria-checked={checked}
    bind:checked
    {disabled}
  />
  <span class="track" aria-hidden="true"><span class="thumb"></span></span>
  {#if label}<span class="label">{label}</span>{/if}
</label>

<style>
  .toggle { display: inline-flex; align-items: center; gap: var(--space-2); cursor: pointer; white-space: nowrap; }
  .toggle-input { position: absolute; opacity: 0; pointer-events: none; }
  .track {
    width: 34px; height: 18px; border-radius: var(--radius-full);
    background: var(--color-border); transition: background var(--duration-fast) var(--ease-out);
    position: relative; flex-shrink: 0;
  }
  .thumb {
    position: absolute; top: 2px; left: 2px; width: 14px; height: 14px; border-radius: 50%;
    background: var(--color-elevated); transition: transform var(--duration-fast) var(--ease-out);
  }
  .toggle-input:checked + .track { background: var(--color-primary); }
  .toggle-input:checked + .track .thumb { transform: translateX(16px); }
  .toggle-input:focus-visible + .track { outline: 2px solid var(--color-primary); outline-offset: 2px; }
  .toggle-input:disabled + .track { opacity: 0.5; cursor: default; }
  .label { font-size: var(--fs-xs); color: var(--color-text); }
</style>
```

说明：视觉上用隐藏原生 checkbox + track/thumb 实现，保留原生键盘与可访问性（`role="switch"` + `aria-checked` + `:focus-visible`）。

### ToolPanel 修改

- `import { t, tMap } from "$lib/i18n";`、`import Select from "./Select.svelte";`、`import Toggle from "./Toggle.svelte";`
- transport：`<Select bind:value={server.transport} options={[{ value: "stdio", label: "stdio" }, { value: "http", label: "http" }]} />`
- method：`<Select bind:value={tool.method} options={[{ value: "GET", label: "GET" }, { value: "POST", label: "POST" }, { value: "PUT", label: "PUT" }, { value: "DELETE", label: "DELETE" }]} />`
- disabled：`<Toggle bind:checked={server.disabled} label={t("toolPanel.disabled")} />`，移除 `.field-toggle` 可收缩（`flex-shrink: 0` + nowrap）。
- 全部文案替换为 `{t("toolPanel.xxx")}`；状态值 `{tMap("toolPanel.status", server.status)}`；错误消息 `t("toolPanel.reassembleFailed", { error: `${e}` })` 等。
- source 标签（native/config/mcp）保留英文原样（技术标识符）。

## Execution Steps / 执行步骤

- Step 1：`translations.ts` 新增 `toolPanel` 命名空间（类型 + en + zh）。
- Step 2：新增 `Toggle.svelte` 组件。
- Step 3：`ToolPanel.svelte` — import i18n/Select/Toggle；transport/method 换 Select；disabled 换 Toggle；全量文案走 `t()`/`tMap()`；修 `.field-toggle` 布局。
- Step 4：验证 — `pnpm check` / `pnpm build`；回写 requirements AC 与 lifecycle。

## Risks / 风险

- `Select` 的浮层在弹窗（overflow: auto 的 `.modal-body`）内会被 portal 到 body，定位用 fixed，需在弹窗场景实际验证（Select 已处理 backdrop + 视口钳制）。
- `tMap("toolPanel.status", ...)` 的 status 键需与后端 `McpServerStatusKind` 的 serde 值（snake_case）一致：connecting / connected / failed / disabled。
- 误译风险：技术标识符（stdio/http/GET/native/config/mcp）保持原文。
