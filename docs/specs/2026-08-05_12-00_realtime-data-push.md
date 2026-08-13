# Spec: 实时数据推送 + 前端统一状态管理

## Goal

修复 agent-app 中「会话列表、对话列表、课题、轮询状态不自动更新」的问题，建立一条端到端的实时数据通道：

1. **后端推送通道**：所有数据变更（会话/课题/轮询/后台推进）通过 Tauri 事件实时推送到前端。
2. **前端统一状态管理**：新增 `dataStore`（Svelte 5 runes 模块级单例，对齐现有 `LayoutStore.svelte.ts` 范式），收敛 `+page.svelte` 的散落状态，事件驱动刷新，消除各面板本地数组与后端不同步。

## Done Contract

1. 后端新增统一事件 `app://state-changed`，负载 `{ kind, ... }`，覆盖 `topics / conversations / poller` 三类变更。
2. 所有前端可写操作（Tauri command）完成后 emit 对应事件；后台 `spawn_poller_runtime` tick 推进后 emit 对应事件。
3. 前端新增 `src/lib/stores/dataStore.svelte.ts`：模块级 `$state` 单例，封装 bootstrap / refresh / subscribe / actions。
4. `+page.svelte` 迁移为组合 `dataStore`，不再直接维护 topics / conversations / pollerStatus 等状态。
5. `SessionList / TopicPanel / PollerPanel` 改为读 `dataStore.state`，本地数组消除。
6. 手动操作（新建/删除/暂停/发送消息）后无需手动刷新即自动更新；后台轮询推进的消息/状态自动出现。
7. 日志 `app://logs` 通道保持不动。

## Scope

**In**:
- 后端：事件常量与负载定义、command 层 emit、`spawn_poller_runtime` 内 emit
- 前端：`dataStore.svelte.ts` 新建、`+page.svelte` 迁移、`SessionList / TopicPanel / PollerPanel` 迁移
- 前端类型对齐（`StateEventKind` 等）

**Out**:
- ~~运行中会话列表（`SessionTracker` 仍无 Tauri 命令暴露，`GetRunningSessionsTool` 为 dead code）——本 spec 不新增 `list_sessions`，会话指"对话列表 conversations"~~ → 已由 `2026-08-05_18-00_gui-running-sessions.md` 承接，本 spec 范围不变（会话指"对话列表 conversations"）
- 事件节流/批处理（tick 高频时不做合并，按实际频度 emit）
- Neuron 面板的自动刷新（`neuron-manager-api` 另起 spec，不在此范围）

## 依赖

- 无新增 crate / npm 包
- 复用现有：`tauri::Emitter`、`@tauri-apps/api/event.listen`、Svelte 5 `$state`

## 设计

### 数据流总览

```
Rust 后端                                  Svelte 前端
─────────                                  ──────────
Tauri command (手动操作)      ──emit──▶  app://state-changed ──listen──▶ dataStore.refresh(kind)
spawn_poller_runtime (后台 tick) ──emit─▶ app://state-changed ──────────▶ dataStore.state.xxx
                                                                          │
                                                       组件 (SessionList/TopicPanel/PollerPanel)
                                                       只读 state + 调用 actions（不再本地缓存）
```

### 一、后端：事件通道

#### 1. 事件名与负载

统一单事件，避免事件爆炸、前端订阅维护成本高：

```rust
/// core/events.rs（新增）
pub const STATE_CHANGED_EVENT: &str = "app://state-changed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum StateChange {
    Topics,                 // topics 变化（列表 / 详情 / 状态）
    Conversations { affected: Vec<String> }, // conversations 变化；affected 为实际写入的会话 id
    Poller { status: PollerStatus }, // poller 状态变化（直接带最新状态，省一次 invoke）
}
```

约定：**"变更后广播、前端重新拉取"** 为主；仅 `Poller` 因数据小且高频，负载直接带 `PollerStatus`。

`Conversations` 携带 `affected`（实际发生写入的会话 id 列表）：

- 前端只重拉**受影响会话**的消息；未受影响会话（如用户正看着 A，后台在推进 B）不重拉、不触发滚动。
- 后台轮询空转（无未完成课题 / 全部跳过）时**不发** `Conversations`/`Topics`，避免无效刷新。

#### 2. emit 注入方式（低侵入）

沿用 `lib.rs:482` 现有 `emit` 闭包模式，扩展为通用事件发射器：

```rust
// lib.rs setup 内
let state_emit = Arc::new(move |change: StateChange| {
    let _ = emit_handle.emit(STATE_CHANGED_EVENT, change);
});
```

- 手动操作路径：在 Tauri command 内完成写操作后调用 `state_emit(...)`（command 可拿到 `AppHandle`）。
- 后台路径：`spawn_poller_runtime` 增加一个 `state_emit` 参数（从 `lib.rs` 传入），tick 与 assistant 推进后调用。

#### 3. 挂载点清单

| 触发点 | 文件 | emit 内容 |
|---|---|---|
| `send_chat_message`（写消息后） | `lib.rs` | `Conversations { affected: [该会话] }` |
| `create_conversation` | `lib.rs` | `Conversations { affected: [新会话] }` |
| `clear_conversation` | `lib.rs` | `Conversations { affected: [被清空会话] }` |
| `create_topic` / `update_topic` / `delete_topic` | `lib.rs` | `Topics` |
| `add_topic_scope_item` / `delete_topic_scope_item` / `complete_topic_scope_item` | `lib.rs` | `Topics` |
| `pause_topic` / `resume_topic` | `lib.rs` | `Topics` |
| `poll_pause` / `poll_resume` / `poll_trigger` | `lib.rs` | `Poller { status }` |
| `spawn_poller_runtime`：每次 `guard.tick()` 后 | `gateway.rs` | `Poller { status }` |
| `spawn_poller_runtime`：assistant step 实际推进后 | `gateway.rs` | `Conversations { affected: 推进的会话 }`（+ `Topics`）；空转不 emit |

> 说明：为最小化侵入，**不在 Store 层注入回调**。command 层 + 后台 runtime 两个 emit 入口已覆盖全部写路径（前端所有写操作都经 command；后台推进都经 `spawn_poller_runtime`）。

### 二、前端：统一状态管理 `dataStore`

#### 1. 文件与结构

新建 `src/lib/stores/dataStore.svelte.ts`，对齐 `LayoutStore.svelte.ts` 范式（模块级 `$state` + 导出单例）：

```ts
// 状态（单一数据源）
const state = $state<DataState>({
  conversations: [],
  activeConversationId: null,
  messages: [],
  topics: [],
  poller: null,          // PollerStatus
  providers: [],
  models: [],
  skills: [],
  ready: false,
});

export const dataStore = {
  state,
  async bootstrap(),            // 首次并行拉取全部（对齐现 +page.svelte onMount）
  async refresh(kind),          // 按 kind 重新 invoke
  subscribe(),                  // listen("app://state-changed") → refresh(kind)；poller 直接写 state
  // actions：内部 invoke + refresh，保证后端与 store 一致
  async createConversation(), async sendMessage(), async clearConversation(),
  async createTopic(), async deleteTopic(), async pauseTopic(), async resumeTopic(),
  async addScopeItem(), async completeScopeItem(), async deleteScopeItem(),
  async pausePoller(), async resumePoller(), async triggerPoller(),
};
```

#### 2. 事件 → 刷新映射

```ts
async function refresh(kind: StateEventKind) {
  switch (kind) {
    case "topics":         state.topics = await invoke("list_topics");
    case "conversations":  await refreshConversations();   // list_conversations + history(active)
    case "poller":         state.poller = await invoke("poll_status");
  }
}
// poller 事件负载直接带 status 时：state.poller = event.payload.status（免一次 invoke）
```

#### 3. 组件迁移

| 组件 | 现状 | 迁移后 |
|---|---|---|
| `+page.svelte` | 668 行上帝组件，持有全部状态 + 一次性 bootstrap | 只留布局 + `dataStore.bootstrap()` + `dataStore.subscribe()`；props 改为读 state |
| `SessionList` | 只读 props，永不过期 | 读 `dataStore.state.conversations` |
| `TopicPanel` | 本地数组，操作后本地改 | 读 `dataStore.state.topics`，操作走 `dataStore` actions |
| `PollerPanel` | 手动 `refresh()` | 读 `dataStore.state.poller`（事件驱动自动更新） |
| `LogPanel` | `listen("app://logs")` | 不动 |

### 三、实施顺序（checkpoint 制）

1. **后端事件通道**：新增 `core/events.rs`（常量 + `StateChange`）；`lib.rs` 建 `state_emit`；command 层挂载（Topic/Poller/Conversation 全部写命令）。
2. **后端后台推进**：`spawn_poller_runtime` 加 `state_emit` 参数，tick 后 emit Poller；assistant 推进后 emit Conversations/Topics。
3. **前端 dataStore**：新建 `dataStore.svelte.ts`（bootstrap / refresh / subscribe / actions）。
4. **前端组件迁移**：`+page.svelte` 瘦身 → `SessionList / TopicPanel / PollerPanel` 改读 state。
5. **验证**：手动操作即时刷新；`poller.enabled=true` 时后台 tick 前端自动跳动；消息自动追加。

## 验收标准

- [ ] `core/events.rs` 定义 `STATE_CHANGED_EVENT` 与 `StateChange`（`Topics/Conversations/Poller{status}`）
- [ ] 全部 Topic 写命令、Conversation 写命令、Poller 控制命令完成后 emit 对应事件
- [ ] `spawn_poller_runtime` tick 后 emit `Poller{status}`；assistant 推进写消息后 emit `Conversations`
- [ ] `dataStore.svelte.ts` 提供 bootstrap / refresh / subscribe / actions，`$state` 单例
- [ ] `+page.svelte` 不再直接持有 topics / conversations / pollerStatus 状态
- [ ] `SessionList / TopicPanel / PollerPanel` 读 `dataStore.state`，无本地数组兜底
- [ ] 手动新建课题/发送消息/暂停轮询后，对应面板无刷新自动更新
- [ ] `poller.enabled: true` 时，PollerPanel 的 tick_count 自动递增
- [ ] `cargo build` 与前端 `check` 通过，无 lint 错误

## 约束

- 业务约束：`Spec is Truth`，`No Spec, No Code`，`No Approval, No Execute`；文档与代码冲突时先同步文档。
- 技术约束：
  - 不引入新依赖；前端状态管理只用 Svelte 5 runes + 模块级单例（对齐 `LayoutStore`）。
  - 后端不绕过 Tauri command 直接访问前端；前端不绕过 command 直写存储。
  - `app://logs` 通道保持不变。
  - 事件 emit 放在 command 层与 `spawn_poller_runtime`，不改 Store 内部签名（最小侵入）。
- 兼容性约束：保持现有 14 个 Topic/Poller command 签名不变，仅新增行为（emit）。

## Open Questions

- [ ] Q1: `Poller` 高频 tick 下是否需要对 `Poller{status}` 事件做节流？当前默认 Paused，频率受 `base_interval_ms` 约束，先不做节流，观察实际表现。
- [ ] Q2: 是否需要 `Topics` 变更时同时携带变更的 topic id（供前端局部刷新而非全量拉取）？当前先全量拉取，数据量小（课题数量级 < 100），若未来量大再优化。
