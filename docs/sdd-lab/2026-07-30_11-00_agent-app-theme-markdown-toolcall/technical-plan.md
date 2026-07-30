# Technical Plan / 技术方案: agent-app-theme-markdown-toolcall

## Requirement Baseline / 需求基线

- 对应需求文档：[requirements.md](file:///d:/work-space/new-start/docs/sdd-lab/2026-07-30_11-00_agent-app-theme-markdown-toolcall/requirements.md)
- 需求确认状态：用户已确认推进
- 本方案覆盖范围：主题切换器 + Markdown 渲染 + 工具调用折叠展示，全部为前端改动，不触 Rust 后端

## Current Project Facts / 当前项目事实

- **已读取文件/模块**：
  - [DESIGN.md](file:///d:/work-space/new-start/DESIGN.md) — 视觉设计规范，定义了 OKLCH 色值体系、排版、间距、组件样式
  - [types.ts](file:///d:/work-space/new-start/packages/agent-app/src/lib/types.ts) — 前端类型定义，`Message` 类型缺少 `tool_calls`/`msg_type` 等字段
  - [ChatMessage.svelte](file:///d:/work-space/new-start/packages/agent-app/src/lib/components/ChatMessage.svelte) — 消息气泡组件，目前仅纯文本展示
  - [StatusBar.svelte](file:///d:/work-space/new-start/packages/agent-app/src/lib/components/StatusBar.svelte) — 顶部状态栏，主题切换器入口将放在此处
  - [app.html](file:///d:/work-space/new-start/packages/agent-app/src/app.html) — HTML 模板，当前无 `data-theme` 属性
  - [+page.svelte](file:///d:/work-space/new-start/packages/agent-app/src/routes/+page.svelte) — 主页面，CSS 变量当前硬编码在 `<style>` 中
  - [package.json](file:///d:/work-space/new-start/packages/agent-app/package.json) — 当前无 marked/dompurify 依赖
- **当前实现事实**：
  - CSS 变量定义在 `+page.svelte` 的 `:global(:root)` 和 `@media (prefers-color-scheme: dark)` 中，使用 hex 色值
  - 主题仅依赖 `prefers-color-scheme` 媒体查询，无手动切换能力
  - `ChatMessage` 直接输出 `message.content` 纯文本，未做任何渲染
  - 前端 `Message` 类型只含 `role/content/timestamp`，缺少 Rust 端实际发送的 `tool_calls`、`msg_type`、`summary_of`、`tool_call_id` 字段
- **相关接口/数据结构**：
  - Rust `Message` 包含 `tool_calls: Option<Vec<ToolCall>>`，`ToolCall { id, name, arguments }`（查看 [models.rs](file:///d:/work-space/new-start/packages/agent-app/src-tauri/src/core/models.rs#L191-L195)）
  - `list_conversations` 返回 `Vec<Conversation>`，其中包含 `messages: Vec<Message>`（完整消息体）
  - `history` 命令也返回 `Vec<Message>`
- **约束与风险**：
  - 不引入额外 UI 组件库，保持 Svelte 5 + 原生 CSS
  - 不引入代码高亮库（highlight.js / Prism）
  - Markdown 渲染需 XSS 防护（DOMPurify）
  - 主题持久化使用 localStorage，不新增 Tauri 插件

## Solution Options / 方案候选

单一方案（三功能均为增量增强，无架构级分歧），不做多方案对比。

### 方案 A（推荐）

- **推荐**：是
- **方案摘要**：三功能并行实现，逐一组件落地。
- **涉及模块**：`types.ts`, `app.html`, `+page.svelte`, `StatusBar.svelte`, `ChatMessage.svelte`, `ChatArea.svelte`；新增 `ThemeSwitcher.svelte`, `MarkdownRenderer.svelte`, `ToolCallBlock.svelte`
- **优点**：改动集中，组件职责清晰，所有改动在前端层
- **缺点**：无
- **风险**：无

## API Design / API 设计

### 前端类型扩展

#### 文件：`src/lib/types.ts`

扩展 `Message` 类型，对齐 Rust 端结构：

```typescript
export type ToolCall = {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
};

export type Message = {
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
  msg_type?: string;
  summary_of?: string[];
  tool_calls?: ToolCall[];
  tool_call_id?: string;
};
```

### CSS 变量重构

原有 CSS 变量从 `+page.svelte` 的 `:global(:root)` 移到 `app.html`，改为 `data-theme` 作用域：

```css
:root, [data-theme="light"] { /* light vars */ }
[data-theme="dark"] { /* dark vars */ }
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]):not([data-theme="dark"]),
  [data-theme="system"] { /* dark vars */ }
}
```

色值对齐 [DESIGN.md](file:///d:/work-space/new-start/DESIGN.md) 的 OKLCH 规范。

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：`pnpm install` 已执行
- 若执行前需求变化：先回写 requirements.md，再继续

### Step 1. 安装依赖

#### 命令

```bash
cd packages/agent-app && pnpm add marked dompurify && pnpm add -D @types/dompurify
```

### Step 2. 重构 CSS 变量体系（主题基础）

#### 文件：`src/app.html`

- 改动类型：修改
- 改动内容：
  1. 添加 `data-theme` 属性支持
  2. 将所有 CSS 变量从 `+page.svelte` 迁移到 `<style>` 或内联 `<script>` 中
  3. 使用 DESIGN.md 的 OKLCH 色值（暖调中性色 + 蓝色强调）
  4. `:root` 默认 light 变量，`[data-theme="dark"]` 深色变量
  5. `prefers-color-scheme` 媒体查询仅在无 `data-theme` 或 `data-theme="system"` 时生效
- 设计约束：遵循 [DESIGN.md](file:///d:/work-space/new-start/DESIGN.md) Theme 章节的完整变量表

#### 文件：`src/routes/+page.svelte`

- 改动类型：修改
- 改动内容：删除 `:global(body)` 和 `:global(:root)` CSS 变量定义（已迁移到 app.html）
- 验收点：应用启动后变量正常应用，深色/浅色均正常

### Step 3. 创建 ThemeSwitcher 组件

#### 文件：`src/lib/components/ThemeSwitcher.svelte`

- 改动类型：新增
- 改动内容：
  1. 下拉选择器：Light / Dark / System
  2. 读取 `localStorage` 的 `theme-preference` 初始化
  3. 选择后设置 `document.documentElement.dataset.theme` 并写入 `localStorage`
  4. System 模式下监听 `matchMedia('prefers-color-scheme')` 变化自动切换
- 设计约束：
  - 放置在 StatusBar 右侧（模型信息旁边）
  - 紧凑设计：一个图标/按钮 + 弹出下拉
  - motion 遵循 DESIGN.md：opacity + translate，150ms ease-out

#### 文件：`src/lib/components/StatusBar.svelte`

- 改动类型：修改
- 改动内容：在 `bar-right` 区域添加 `<ThemeSwitcher />`
- 验收点：状态栏右侧显示主题切换入口，切换后即时生效

### Step 4. 扩展前端类型

#### 文件：`src/lib/types.ts`

- 改动类型：修改
- 改动内容：
  1. 新增 `ToolCall` 类型
  2. 扩展 `Message` 类型添加可选字段 `msg_type`, `summary_of`, `tool_calls`, `tool_call_id`
- 验收点：类型对齐 Rust `Message` 结构，无编译错误

### Step 5. 创建 MarkdownRenderer 组件

#### 文件：`src/lib/components/MarkdownRenderer.svelte`

- 改动类型：新增
- 改动内容：
  1. 接收 `content: string` prop
  2. 使用 `marked` 解析 Markdown 为 HTML
  3. 使用 `DOMPurify.sanitize()` 过滤输出
  4. 通过 `{@html sanitizedHtml}` 渲染
  5. 支持元素：h1-h6, p, ul/ol, code block (with lang label), inline code, strong/em, a, hr, table, blockquote
  6. 代码块渲染：使用纯 CSS 样式
  7. 链接在新标签页打开（target="_blank" rel="noopener noreferrer"）
- 设计约束：
  - 代码块样式纯 CSS（深色背景 + 等宽字体 + 语言标签）
  - 表格列过多时水平滚动
  - 不引入代码高亮库
- 验收点：各类 Markdown 元素正确渲染，XSS 被过滤

### Step 6. 更新 ChatMessage 组件

#### 文件：`src/lib/components/ChatMessage.svelte`

- 改动类型：修改
- 改动内容：
  1. 引入 `MarkdownRenderer` 组件
  2. assistant 和 user 角色的 `message.content` 使用 `MarkdownRenderer` 渲染
  3. system 角色保持纯文本（不做渲染）
  4. 如果 `message.tool_calls` 存在，在内容下方渲染 `<ToolCallBlock toolCalls={message.tool_calls} />`
  5. 调整样式以适应渲染后的内容（content 不再需要 `white-space: pre-wrap`）
- 设计约束：DESIGN.md 中 Chat bubble 的样式规范

### Step 7. 创建 ToolCallBlock 组件

#### 文件：`src/lib/components/ToolCallBlock.svelte`

- 改动类型：新增
- 改动内容：
  1. 接收 `toolCalls: ToolCall[]` prop
  2. 默认折叠状态，显示摘要行：工具名称列表
  3. 点击展开，显示每个 tool 的详情：
     - 工具名称（突出显示）
     - 参数（JSON 格式化，`<pre>` 展示）
     - 执行结果或 `tool_call_id`
  4. 多 tool 列表展示
  5. 折叠/展开有 smooth 动画（opacity + max-height）
- 设计约束：
  - 与普通气泡用侧边指示条或背景色区分
  - 展开区高度限制 + 滚动
  - 使用 `--color-surface` 背景，左侧 `--color-border` 竖线指示

### Step 8. 更新 ChatArea 组件

#### 文件：`src/lib/components/ChatArea.svelte`

- 改动类型：修改
- 改动内容：消息内容 CSS 从 `white-space: pre-wrap; word-break: break-word;` 调整，因为 Markdown 渲染后不再需要 `pre-wrap`（但纯文本仍需保留换行）
- 验收点：消息流展示正常，无布局错位

### Step 9. 检查与回写

#### 文件：`docs/sdd-lab/2026-07-30_11-00_agent-app-theme-markdown-toolcall/lifecycle.md`

- 回写执行记录：
- 记录实际改动摘要：
- 记录验证结果：
- 记录下一步状态：

## Risk And Mitigation / 风险与缓解

- 风险：DOMPurify 在 SvelteKit SPA 模式下的导入兼容性
  - 缓解方式：Vite 会自动 polyfill node 模块，`dompurify` 可在浏览器环境正常使用；如有问题使用 `import DOMPurify from 'dompurify'` 标准导入即可
- 风险：`marked` 解析大型 Markdown 时的性能
  - 缓解方式：`marked.parse` 是同步操作，消息内容通常 <50KB，无性能风险
- 风险：CSS 变量迁移导致现有组件颜色错乱
  - 缓解方式：变量名保持 `--color-*` 不变，仅改变值的写法（hex → oklch）和作用域位置

## Execute Checkpoint / 执行检查点

- **当前理解**：需要实现三个前端增强功能——主题切换器（手动 light/dark/system + localStorage 持久化）、Markdown 渲染（marked + DOMPurify）、工具调用折叠展示。所有功能基于已有 CSS 变量体系和 Rust Message 数据结构的 tool_calls 字段，不触后端。
- **核心目标**：完成三个组件的创建和现有组件的修改，使 GUI 在消息展示丰富度和视觉控制上超越 TUI。
- **下一步动作**：
  1. `pnpm add marked dompurify` 安装依赖
  2. 重构 CSS 变量到 app.html（data-theme 作用域），对齐 DESIGN.md OKLCH 色值
  3. 创建 ThemeSwitcher 组件，嵌入 StatusBar
  4. 扩展 types.ts Message 类型
  5. 创建 MarkdownRenderer 组件
  6. 创建 ToolCallBlock 组件
  7. 更新 ChatMessage / ChatArea 组件
  8. 验证全部验收标准
- **风险**：无显著风险。DOMPurify 在 SvelteKit SPA 下需验证导入，但已规划标准方案。
- **验证方式**：
  - 主题切换：手动切换三个模式，刷新后保持
  - Markdown 渲染：发送含 Markdown 语法的消息（由 TUI 的 echo 命令回显），验证渲染结果
  - 工具调用：验证带有 tool_calls 的 Message 显示折叠块
