# Spec: GUI 运行中会话展示

## Goal

补上「前端看不到对话正在运行中」的能力：将 `SessionTracker`（纯内存运行中会话追踪）通过 Tauri 命令暴露给 GUI，并在后台 poller 推进路径中正确注册/注销会话，让前端会话列表能实时显示「运行中」标记。

## 背景

- 设计文档（TUI 时代 `runtime-manager.md` 的 `[Running]` 标记、GUI `requirements.md` 的「关闭运行中会话」）有涉及，但最新 `realtime-data-push.md` 曾将本能力列为 Out。
- 根因 1：`SessionTracker` 无 Tauri 命令暴露（`GetRunningSessionsTool` 为 dead code）。
- 根因 2：后台 `process_step_request`（`step_poller` 推进）**不经过** `send_model_message`/`assistant_step`，所以后台运行会话从未被 register，即使暴露命令也拿不到。

## Done Contract

1. 后端新增 `list_running_sessions` 命令，返回 `Vec<RunningSession>`（含 `session_id / started_at / current_step`）。
2. `RunningSession` 增加 `Serialize`；`SessionTracker` 支持 `set_on_change` 回调，register/unregister/update_step/close 后触发。
3. `StateChange` 增加 `Sessions` 变体；`close_session` 命令改为 emit `Sessions`。
4. 后台 `process_step_request` 的 PollAll 分支：对每个待推进 session 先 `register` 再 `step_poller`，完成后 `unregister`。
5. 前端 `dataStore` 增加 `runningSessions` 状态、`refreshRunningSessions()` 与 `sessions` 事件分支（bootstrap 并行拉取）。
6. `SessionList` 对运行中的会话显示脉冲「●」标记（对齐 TUI 的 `[Running]` 语义）。

## Scope

**In**:
- 后端：`session_tracker.rs`（Serialize + on_change）、`events.rs`（Sessions 变体）、`assistant_mode.rs`（poller 推进注册）、`gateway.rs`（注入回调）、`lib.rs`（新命令 + 注册）
- 前端：`types.ts`（RunningSession）、`dataStore.svelte.ts`、`SessionList.svelte`

**Out**:
- 运行中会话的「当前步骤」文本展示（`current_step` 字段随命令返回，UI 暂不渲染）
- 会话结束通知 / 运行中会话的关闭按钮视觉强调
- 事件节流/批处理

## 设计

### 一、后端

#### 1. `session_tracker.rs`

```rust
#[derive(Debug, Clone, Serialize)]
pub struct RunningSession {
    pub session_id: String,
    pub started_at: u128,
    pub current_step: Option<String>,
}
```

`SessionTracker` 增加 `on_change: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>`：

- `set_on_change(callback)`：注入变更回调（由 `gateway.rs` 在 `with_state_emitter` 中注入，指向 `emit(StateChange::Sessions)`）。
- 内部 `notify()`：register / unregister / update_step / close 成功后（锁外）调用。

#### 2. `events.rs`

```rust
pub enum StateChange {
    Topics,
    Conversations,
    Poller { status: PollerStatus },
    Sessions,   // 运行中会话集合变化，前端重新拉取
}
```

#### 3. `gateway.rs`

`with_state_emitter` 内构造 `SessionTracker` 后：

```rust
let session_tracker = SessionTracker::new();
if let Some(emit) = state_emit.as_ref() {
    let emit = Arc::clone(emit);
    session_tracker.set_on_change(Arc::new(move || {
        emit(StateChange::Sessions);
    }));
}
```

`AssistantMode::new` 增加 `session_tracker` 参数（clone 传入）。

#### 4. `assistant_mode.rs`

`AssistantMode` 增加 `session_tracker` 字段。`process_step_request` 的 PollAll 分支对每个 session：

```rust
if let Err(error) = self.session_tracker.register(&session_id, None) {
    eprintln!("assistant poll register failed for {}: {error}", topic.id);
    continue;
}
let _ = self.session_tracker.update_step(&session_id, "polling");
if let Err(error) = self.step_poller(&session_id, model).await {
    eprintln!("assistant poll step failed for {}: {error}", topic.id);
}
self.session_tracker.unregister(&session_id);
```

#### 5. `lib.rs`

```rust
#[tauri::command]
async fn list_running_sessions(
    sessions: State<'_, SessionTracker>,
) -> TauriResult<Vec<RunningSession>> {
    sessions.inner().list().map_err(|error| error.payload())
}
```

`close_session` 的 emit 由 `Conversations` 改为 `Sessions`（关闭后 running 集合变化；会话列表本身不变）。命令注册进 `generate_handler!`。

### 二、前端

#### 1. `types.ts`

```ts
export type RunningSession = {
  session_id: string;
  started_at: number;
  current_step: string | null;
};
```

#### 2. `dataStore.svelte.ts`

- `StateEventKind` / `StateChangePayload` 增加 `"sessions"`。
- `state.runningSessions: RunningSession[]`。
- `refreshRunningSessions()`：`invoke("list_running_sessions")`。
- `handleStateChanged` 增加 `sessions` 分支；`bootstrap` 并行拉取；导出 `refreshRunningSessions`。

#### 3. `SessionList.svelte`

`$derived` 计算 `runningSessionIds` 集合，命中会话的 `session-id` 旁显示脉冲 `●`（`--color-success` + 呼吸动画）。

## 实施顺序

1. 后端 `session_tracker.rs` / `events.rs` / `gateway.rs` / `assistant_mode.rs` / `lib.rs`
2. 前端 `types.ts` / `dataStore.svelte.ts` / `SessionList.svelte`
3. `cargo check` / `cargo test` 与前端 `check` 验证

## 验收标准

- [ ] `list_running_sessions` 命令注册并可调用，返回运行中会话（空闲时为空数组）
- [ ] 后台 poller 推进（PollAll）期间，目标会话出现在 running 列表；推进结束即消失
- [ ] 前端收到 `sessions` 事件后自动刷新 running 列表
- [ ] 会话列表中运行中的会话显示脉冲「●」标记
- [ ] `cargo build` 与前端 `check` 通过，无 lint 错误

## 约束

- 不引入新依赖。
- `SessionTracker` 的变更通知不改变其 TUI 使用方式（`set_on_change` 为可选项，默认无回调）。
- `close_session` 语义不变（强制关闭运行中会话并触发 abort）。
