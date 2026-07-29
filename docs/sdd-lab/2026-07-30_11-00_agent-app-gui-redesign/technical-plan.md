# Technical Plan / 技术方案: agent-app-gui-redesign

## Requirement Baseline / 需求基线

- 对应需求文档：`docs/sdd-lab/2026-07-30_11-00_agent-app-gui-redesign/requirements.md`
- 需求确认状态：已确认（Q1-Q4 + TUI 对齐缺口 + 约束/模式调整）
- 本方案覆盖范围：Rust 后端新增 Tauri 命令 + Svelte 前端组件化重构与功能完善。不涉及 Neuron/Topic/Poller 管理界面、token streaming、会话 compaction。

## Current Project Facts / 当前项目事实

- 已读取文件/模块：
  - `src-tauri/src/lib.rs` — Tauri 命令注册中心（9 个命令）
  - `src-tauri/src/core/gateway.rs` — Gateway 主入口（20+ 公共方法）
  - `src-tauri/src/core/models.rs` — 数据模型（Conversation, ConversationMode, ChatOptions 等）
  - `src/routes/+page.svelte` — 当前单页面前端（内联组件、无拆分）
  - `src/routes/+layout.ts` — SPA 模式配置
  - `src/app.html` — HTML 模板
  - `src-tauri/src/tui/app.rs`, `commands.rs`, `event.rs` — TUI 参考实现
- 当前实现事实：
  - 前端为单页面 Svelte，无组件拆分，状态全在 `+page.svelte`
  - 发送消息使用 `gateway.send_message()`，内部只处理 `/echo` 和 `/time`，不调用真实 LLM（实际上是占位逻辑）
  - 会话模式（Chat/Agent/Assistant）Rust 端已有支持，但前端 TypeScript 类型 `Conversation` 未包含 `mode` 字段
  - 缺乏输入历史、键盘快捷键、会话切换等交互能力
- 相关接口/数据结构：
  - `Conversation { id, mode, messages, created_at, updated_at }` — mode 字段已 serde 序列化，前端只需补充类型
  - `ConversationMode { Chat, Agent, Assistant }` — 字符串序列化
  - `ChatOptions { provider_id, model_id, conversation_id }` — send_model_message 参数
  - `RuntimeStatus { app_name, storage_path, current_conversation_id, skill_count, conversation_count }`
- 约束与风险：
  - 新增 Tauri 命令仅限于 Gateway 现有方法封装，无后端新逻辑
  - 不引入 UI 组件库，使用原生 CSS Grid/Flexbox
  - `send_message` 当前是同步 mock，需替换为 async 调用 `send_model_message`
  - Tauri `State<Mutex<Gateway>>` 在 async 命令中需 clone 后释放锁

## Open Questions / 开放问题

无。全部需求已在需求文档中确认。

> 技术方案层面无非自行判断的内容。现有 Gateway 方法签名明确、前端技术栈确定、约束条件清晰，无需向用户提问。

## Solution Options / 方案候选

### Option A / 方案 A（推荐）

- 推荐：是
- 方案摘要：组件化重构 + 新增 3 个 Tauri 命令 + 替换 send_message 为真实 LLM 调用
- 涉及模块：`lib.rs`（Rust 命令）、`+page.svelte`（主布局）、新组件（8 个 Svelte 组件）
- 优点：
  - 最小改动量：后端只加 3 个命令，前端拆分组件但保持单页面架构
  - 与 TUI 共享 Gateway 能力层，无需复制业务逻辑
  - Svelte 5 runes（$state, $derived, $effect）天然支持响应式状态管理
- 缺点：
  - 需要重写 send_message 为 async，涉及 Mutex 锁的异步模式变化
- 风险：
  - 低。Gateway 接口已成熟，TUI 已验证过相同调用路径

### Option B / 方案 B

- 推荐：否
- 方案摘要：引入 UI 组件库（如 shadcn-svelte）进行重构
- 优点：组件库提供现成的 dialog、select、sidebar 等组件
- 缺点：违反需求约束"不引入额外 UI 组件库"；增加依赖体积和构建复杂度

## Decision / 方案决策

- Selected / 选定方案：Option A
- Why / 选择原因：最小改动、满足所有约束、与现有技术栈一致
- Decision Owner / 决策人：推荐方案，等待用户确认
- Decision Time / 决策时间：
- Open Questions 状态：全部关闭

## API Design / API 设计

### Contract Scope / 契约范围

- 变更类型：扩展
- 消费方：Svelte 前端
- 真相源文件：`src-tauri/src/lib.rs`, `src/routes/+page.svelte`

### 新增 Tauri 命令

#### `create_conversation`

```rust
#[tauri::command]
fn create_conversation(
    state: State<'_, Mutex<Gateway>>,
    mode: String,  // "chat" | "agent" | "assistant"
) -> TauriResult<String>  // returns new conversation_id
```

- 底层调用：`gateway.create_new_conversation(ConversationMode)`
- 输入：`mode` 字符串，前段映射为 `ConversationMode` 枚举
- 输出：新建会话的 ID

#### `send_chat_message`（替换现有 `send_message`）

```rust
#[tauri::command]
async fn send_chat_message(
    state: State<'_, Mutex<Gateway>>,
    message: String,
    provider_id: String,
    model_id: String,
    conversation_id: Option<String>,
) -> TauriResult<ChatResponse>
```

- 底层调用：`gateway.send_model_message(input, ChatOptions{provider_id, model_id, conversation_id})`
- 异步命令，需 clone gateway 后释放 Mutex 锁
- 替换已有的同步 `send_message` 命令（不再使用 `/echo`/`/time` mock）

#### `close_session`

```rust
#[tauri::command]
fn close_session(
    state: State<'_, Mutex<Gateway>>,
    session_id: String,
) -> TauriResult<String>
```

- 底层调用：`gateway.session_tracker().close(&session_id)`
- 输出：关闭结果消息

### 需调整的现有命令

无破坏性变更。`send_message` 替换为 `send_chat_message` 后，旧的 `send_message` 可删除。

### 前端 TypeScript 类型补充

```typescript
// Conversation 补充 mode 字段
type Conversation = {
    id: string;
    mode: "chat" | "agent" | "assistant";  // 新增
    messages: Message[];
    created_at: number;
    updated_at: number;
};
```

## Execution Steps / 执行步骤

### Step 0. 执行前检查

- 前置条件：用户确认选定 Option A
- 若执行前需求、API、范围或交互规则变化：暂停并回写文档

### Step 1. Rust 后端 — 新增 Tauri 命令

#### 文件：`src-tauri/src/lib.rs`

- 改动类型：修改
- 改动内容：
  1. 新增 `create_conversation` 命令（同步，调用 `gateway.create_new_conversation`）
  2. 新增 `send_chat_message` 命令（async，替换现有 `send_message`，调用 `gateway.send_model_message`）
  3. 新增 `close_session` 命令（同步，调用 `gateway.session_tracker().close`）
  4. 删除旧 `send_message` 命令
  5. 更新 `invoke_handler` 注册
- 设计约束：
  - async 命令的 Mutex 处理参照现有 `call_model` 模式：`let gateway = state.lock()...clone()`
  - `create_conversation` 需要 `&mut self`，保持同步
- 验收点：
  - `cargo check --manifest-path src-tauri/Cargo.toml --bins` 零错误
  - 新增命令编译后可通过前端 invoke 调用

### Step 2. 前端 — 创建组件文件结构

#### 文件：`src/lib/components/`（新建目录）

```
src/lib/components/
  ChatMessage.svelte       # 单条消息展示
  ChatArea.svelte          # 消息列表 + 自动滚动
  ChatInput.svelte         # 输入框 + 输入历史
  SessionList.svelte       # 会话列表侧栏
  SessionCreateModal.svelte # 新建会话模式选择弹窗
  ModelBar.svelte          # 顶部模型选择下拉
  SidePanel.svelte         # 侧栏（Provider/Model/Skill 信息）
  StatusBar.svelte         # 顶部状态栏
  ErrorBanner.svelte       # 错误提示
```

#### 文件：`src/lib/`（可选，如果有共享工具）

- 如有必要，可在 `src/lib/` 下创建 `types.ts` 用于共享类型定义
- 改动类型：新增

### Step 3. 前端 — 实现组件

#### 文件：`src/lib/components/ChatMessage.svelte`

- 改动类型：新增
- 改动内容：
  - 接收 `message: { role, content, timestamp }` 和 `mode` prop
  - 根据 role（user/assistant/system）渲染不同样式
  - user：右对齐，蓝色气泡风格
  - assistant：左对齐，灰色/浅色气泡风格
  - system：居中窄条，斜体
- 验收点：可展示三种角色消息，视觉区分明显

#### 文件：`src/lib/components/ChatArea.svelte`

- 改动类型：新增
- 改动内容：
  - 接收 `messages[]` prop，遍历渲染 `ChatMessage`
  - 自动滚动到底部（新消息到达时）
  - 支持键盘滚动（↑↓/PgUp/PgDn）
  - 加载中状态：置底显示 "Thinking..." 指示器
- 验收点：消息列表滚动正确，新消息自动追底

#### 文件：`src/lib/components/ChatInput.svelte`

- 改动类型：新增
- 改动内容：
  - `<textarea>` 输入框，支持多行
  - Enter 发送，Shift+Enter 换行
  - ↑↓ 键遍历输入历史（数组存储最近 N 条已发送消息）
  - 发送时 disabled + 加载状态
  - 发送成功后清空输入框，历史指针复位
- 验收点：Enter 发送、Shift+Enter 换行、↑↓ 回溯历史

#### 文件：`src/lib/components/SessionList.svelte`

- 改动类型：新增
- 改动内容：
  - 接收 `sessions[]`、`activeId` 和 `onSelect`、`onCreate`、`onClose` 回调
  - 默认展开侧栏，< 800px 时自动收起为图标按钮
  - 每条会话显示：截短 ID 或自定义名、消息数、模式标签（Chat/Agent/Assistant badge）
  - 当前活跃会话高亮
  - 底部"新建会话"按钮
  - 每条会话悬停显示"关闭"按钮（Assistant 模式）
  - 空状态："暂无会话，创建一个" 引导
- 验收点：列表渲染、点击切换、新建/关闭操作有效

#### 文件：`src/lib/components/SessionCreateModal.svelte`

- 改动类型：新增
- 改动内容：
  - 弹窗展示三种模式选项（Chat / Agent / Assistant）
  - 每种模式带简要说明（Chat=普通对话, Agent=可调工具, Assistant=自主推进）
  - 点击任一模式 → dispatch('create', mode) → 父级调用 create_conversation
  - Esc 或点击遮罩关闭
- 验收点：弹窗展示、模式选择后触发创建

#### 文件：`src/lib/components/ModelBar.svelte`

- 改动类型：新增
- 改动内容：
  - 两个下拉选择器：Provider + Model
  - Provider 选中后，Model 列表自动过滤为该 provider 的模型
  - 模型选项展示 ID + capabilities 标签
  - 选中后 dispatch('modelChange', { providerId, modelId })
- 验收点：联动筛选正确，选中后顶部状态更新

#### 文件：`src/lib/components/SidePanel.svelte`

- 改动类型：新增
- 改动内容：
  - 选项卡切换：Providers / Models / Skills
  - Providers 标签：列表展示 ID、display_name、auth_env、api_base、kind
  - Models 标签：列表展示 ID、provider_id、capabilities（带 ✓ 标签）、context_window、pricing
  - Skills 标签：列表展示 name + description
- 验收点：三个标签页切换正常，数据展示完整

#### 文件：`src/lib/components/StatusBar.svelte`

- 改动类型：新增
- 改动内容：
  - 应用名称（Agent App）
  - 当前会话 ID 截短显示 + 模式标签（Chat/Agent/Assistant）
  - 当前活跃模型（provider_id/model_id）
- 验收点：顶部信息与当前状态一致

#### 文件：`src/lib/components/ErrorBanner.svelte`

- 改动类型：新增
- 改动内容：
  - 接收 `message` + `details`（可选原因/建议）
  - 红色/橙色 banner 风格，右上角关闭按钮
  - 5 秒后自动消失，或手动关闭
- 验收点：错误显示正确，关闭正常

### Step 4. 前端 — 重写主页面

#### 文件：`src/routes/+page.svelte`

- 改动类型：修改（重写）
- 改动内容：
  - 引入所有子组件
  - 使用 Svelte 5 `$state` 管理全局状态：
    - `conversations`, `activeConversationId`
    - `activeProviderId`, `activeModelId`
    - `messages[]`（当前会话消息）
    - `providers`, `models`, `skills`
    - `error`
  - 使用 `$effect` 监听 activeConversationId 变化 → 自动调用 `history()` 加载消息
  - 布局：CSS Grid 三区域（顶栏 | 侧栏 + 主区 | 错误条）
  - 快捷键绑定：Tab 焦点切换、Ctrl+J 新建会话、Esc 关闭弹窗
  - 侧栏收起状态管理
  - 删除旧的无会话"模型调用"面板
- 设计约束：
  - 不引入 Svelte stores 或外部状态库，仅用 runes
  - 布局使用 CSS Grid：`grid-template-areas: "status status" "sidebar main" "error error"`
- 验收点：整体布局正确，组件交互联动正常

### Step 5. 样式与响应式

#### 文件：`src/routes/+page.svelte`（`<style>` 块）

- 改动类型：修改
- 改动内容：
  - 全局 CSS 变量复用：`--color-bg`, `--color-surface`, `--color-text`, `--color-primary`
  - 系统深色/浅色模式通过 `@media (prefers-color-scheme: dark)` 适配
  - 侧栏固定宽度 280px，< 800px 时收起为图标按钮
  - 响应式布局：主区域 min-width 320px，避免内容溢出
- 验收点：窗口缩放时布局不乱，深色模式自动切换

### Step N. 检查与回写

#### 命令

- `cd packages/agent-app && pnpm check` — Svelte 类型检查
- `cargo check --manifest-path src-tauri/Cargo.toml --bins` — Rust 编译检查
- `cd packages/agent-app && pnpm tauri build` — 完整构建（可选验证）

#### 文件：`docs/sdd-lab/2026-07-30_11-00_agent-app-gui-redesign/lifecycle.md`

- 回写执行记录
- 记录实际改动摘要
- 记录验证结果
- 更新状态至 `done`

## Risk And Mitigation / 风险与缓解

- 风险：Tauri async 命令中 `State<Mutex<Gateway>>` 的锁竞争
  - 缓解方式：参照现有 `call_model`/`send_chat_message` 模式——先 lock 后 clone，释放锁再 await。Gateway 目前所有 async 方法都是 `&self`（不可变），clone 后无竞态风险。
- 风险：Svelte 5 runes（$state/$derived/$effect）为新语法，与 Svelte 4 差异大
  - 缓解方式：当前项目已使用 Svelte 5（`svelte: ^5.0.0`），`+page.svelte` 已使用 `$state`。沿袭现有写法即可，不引入兼容层。

## Execute Checkpoint / 执行检查点

- 当前理解：技术方案已完成，覆盖 Rust 后端 3 个新命令 + Svelte 前端 9 个新组件 + 主页面重写
- 核心目标：确认方案后进入执行；执行完成后 GUI 功能覆盖度追上 TUI 且体验超越终端
- 下一步动作：用户审阅本方案并确认执行
- 风险：低。所有改动基于现有 Gateway 接口，TUI 已验证过相同调用路径
