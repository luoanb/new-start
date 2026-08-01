# Spec: Gateway 长锁解冻（分域持锁 + UI 不卡死）

## Goal

- 要解决什么问题：长任务（bootstrap / converse / call_model）持有整颗 `Mutex<Gateway>` 跨网络 I/O，叠加同步 Tauri command 的 `blocking_lock`，导致桌面弹出「无响应，退出/等待」。
- 验收结果：启动 bootstrap 与 Assistant 长对话期间窗口可操作、不弹系统无响应；Tauri 按域持有 Arc，命令不再挤一把上帝锁跨网络。

## Done Contract

- 什么算完成：
  1. **永不持 Gateway/Meta 锁跨网络**（bootstrap、converse、`call_model`、ensure 补齐均 clone-out 后 await）。
  2. **Tauri 分域 `manage`**：至少独立暴露 `NeuronManager`、`TopicStore`、`AssistantMode`、`Poller`、薄 `GatewayMeta`（含 `current_conversation_id` 等）；命令按域取状态。
  3. 首屏/读路径 command 为 `async` + 短 `.lock().await`（或只锁已 clone 的域），消除对长临界区的 `blocking_lock` 死等。
  4. `docs/agent-app/architecture.md` 写明锁规则；`cargo test --lib` 通过；手工冷启动 + 发一条 Assistant 消息期间无系统「无响应」。
- 由什么证明：代码审查（无「持 Gateway 锁 `.await` 模型」）+ 测试输出 + 手工窗口响应。
- 哪些情况仍算未完成：模型调用本身耗时优化；完整 actor 总线；前端去掉 Loading 门闩（可选，非本契约必需）。

## Scope

- In:
  - [`packages/agent-app/src-tauri/src/lib.rs`](../../packages/agent-app/src-tauri/src/lib.rs) — setup、commands、state manage
  - [`packages/agent-app/src-tauri/src/core/gateway.rs`](../../packages/agent-app/src-tauri/src/core/gateway.rs) — 拆短临界区 / 元状态
  - 必要的 `Engine` 并发包装（`Arc<Mutex<Engine>>` 或 `&self` 化）
  - [`docs/agent-app/architecture.md`](../agent-app/architecture.md) 锁与并发节
- Out:
  - 神经元创建批量化（已另修）
  - 把 bootstrap 改成前端手动触发
  - 全量 actor / 消息总线重构
  - 前端 Loading UX 大改（允许保留 `ready` 门闩，但不得因锁而分钟级等待）

## Facts / Constraints

- 已确认事实：
  - `NeuronManager` / `AssistantMode` / `TopicStore` / `Poller` / `SessionTracker` 内部已是 Arc 或内含 Arc；poller 已在 Gateway 锁外跑。
  - 当前瓶颈：Tauri `Mutex<Gateway>` 包住几乎所有命令；setup bootstrap 与 `send_chat_message` 持锁跨模型调用。
  - 前端 `Promise.all(invoke…)` **不阻塞 JS 渲染线程**；系统「无响应」来自原生侧 `blocking_lock` 堵 OS 线程。
  - 共享 SQLite：`TopicStore` / `NeuronStore` 共用 `app.db` Connection Arc，拆外层锁后争用下沉到 conn。
- 技术约束：
  - 跨域短锁顺序固定：`meta → topic → neuron`（需要多锁时），避免死锁。
  - 用户已批准执行（2026-08-01）。
- 已知风险：
  - `current_conversation_id` 并发写需收拢到 Meta。
  - `Engine::chat(&mut self)` 若不改，并行 chat 需单独互斥。
  - 分域 manage 接线面大，须一次做完避免半迁移双轨。

## Open Questions

- [x] Q1 阶段 1（止血）与阶段 2（分域）是否分迭代？
  - **否。用户要求一起做。** 单次交付：clone-out 长路径 + 分域 manage。
- [x] Q2 前端是否必须去掉 Loading / `ready` 门闩？
  - **否。** 契约只要求 invoke 不被长锁拖成分钟级；门闩可保留。

## Restated Understanding

- 我理解当前任务是：系统性解除 Gateway 上帝锁导致的桌面无响应，并一次落地分域持锁。
- 当前核心目标是：锁不住网络；命令按域抢短锁；bootstrap/对话与 UI 解耦。
- 当前边界是：Tauri + Gateway 装配与并发模型；不改 LLM 耗时、不做全量 actor 总线。
- 暂不处理：Loading UX 重设计；TUI/CLI 除共享 core 外的独立锁模型（它们无外层 tokio Mutex，受益于 core 侧拆分即可）。

## Community Survey / Tauri 社区方案对比

调研来源（2026-08）：[Tauri v2 State Management](https://v2.tauri.app/develop/state-management/)、[tokio/tauri Mutex 文档](https://docs.rs/tauri/latest/tauri/async_runtime/struct.Mutex.html)、[Discussion #6531](https://github.com/tauri-apps/tauri/discussions/6531)、[Simon Hyll State Management](https://tauri.by.simon.hyll.nu/concepts/tauri/state_management/)、社区文章（DEV: shipped-apps / async pitfalls）、[actor + mpsc 实践](https://github.com/dardourimohamed/tauri-background-service/blob/main/ARCHITECTURE.md)、[Calling the Frontend](https://v2.tauri.app/develop/calling-frontend/)。

### 方案 A — `std::sync::Mutex` + 放锁再 await（clone-out）

社区口径：官方与多数作者**首选**。数据类状态用 `std::Mutex`；在 `.await` 前结束 guard（常复制/`Arc::clone` 出句柄）。

| 优点 | 缺点 |
| --- | --- |
| 性能好；不易把整棵状态锁穿网络 | 要改调用结构，长路径需显式拆「准备 / await / 收尾」 |
| 与 Tokio「别无故持 async mutex」一致 | 漏一处仍可能卡死 |
| 适配已有 `Arc<NeuronManager>` 等 | — |

### 方案 B — `tokio::sync::Mutex` 持锁跨 await

社区口径：#6531 等允许「必须持锁跨 await」时用；Tokio 文档强调主要给 **IO 资源**，且更贵。

| 优点 | 缺点 |
| --- | --- |
| 改动小：async command 里 `lock().await` 后直接调模型 | **正是当前踩坑形态**：持锁跨网络 → 其它 IPC 全堵 → 系统「无响应」 |
| 编译器对 Send 更友好 | 与 sync `blocking_lock` 混用更糟 |
| — | 社区明确：能放锁就不要用这条当默认 |

### 方案 C — 分域 `manage`（多 State，细粒度）

社区口径：官方 `manage` 可挂多个类型；按能力拆 State，命令只注入所需域。

| 优点 | 缺点 |
| --- | --- |
| 读课题不跟 bootstrap 抢同一把上帝锁 | 接线/生命周期改动面大 |
| 与 poller「域外运行」一致 | 跨域事务要约定锁序 |
| 读多写少域可再加 `RwLock` | 半迁移期易双轨 |

### 方案 D — Actor / mpsc（单任务拥有可变状态）

社区口径：Tokio Mutex 文档建议 IO 资源用「spawn 任务 + 消息传递」；后台服务类项目常用 `mpsc` actor。

| 优点 | 缺点 |
| --- | --- |
| 无共享可变锁争用；串行化清晰 | 重构成本最高；每个命令变消息 |
| 长任务天然在 actor 内，不堵 IPC 线程 | 延迟/背压要设计；调试链路变长 |
| — | 对本仓库「已有多 Arc 子系统」偏重 |

### 方案 E — 后台 `spawn` + `AppHandle` + `emit` 进度（辅）

社区口径：长任务用事件/Channel 推前端；`AppHandle` 廉价 clone 取 state。

| 优点 | 缺点 |
| --- | --- |
| UI 可显示 bootstrap 进度；不挡首屏 | **只解决通知，不解决锁持有**；须与 A/C 组合 |
| 官方 Calling Frontend 一等公民 | 多一套事件协议 |

### 对照本仓库选型

| 方案 | 能否单独解决「无响应」 | 本仓库采纳 |
| --- | --- | --- |
| A clone-out | 能（长路径） | **必做** |
| B 持锁跨 await | 否（加重） | **弃用为默认**；仅内层 SQLite 等短 IO 可酌情 |
| C 分域 manage | 能（结构性） | **必做**（与 A 同批） |
| D Actor | 能（最干净） | **Out**（成本过高；后续可局部用于单一 IO 资源） |
| E emit 进度 | 不能单独 | **可选增强**（实现后可加 bootstrap 进度事件，非 Done Contract 必需） |

**结论（与 Done Contract 对齐）**：社区主流是 **A（放锁）优先于 B（持锁）**；结构性用 **C（分域 State）**；**D** 留给更重的后台服务；**E** 改善体验但不替代放锁。本 spec 单次交付 = **A + C**。

## 接口契约设计

### 硬规则

```text
RULE 1  Never hold GatewayMeta / 域锁 across network I/O
RULE 2  Clone-out then await（短锁 Arc::clone → drop → 长任务）
RULE 3  禁止 sync command 对可能被长任务占用的锁使用 blocking_lock 死等
RULE 4  跨域加锁顺序：meta → topic → neuron（→ engine 若需要）
```

### Tauri 分域 State（目标）

```rust
// 伪代码：setup 时分别 manage
struct GatewayMeta {
    current_conversation_id: Mutex<String>, // 或 tokio::sync::Mutex
    // 仅元状态；不含 NeuronManager / TopicStore 本体
}

app.manage(neuron_manager);              // Arc<NeuronManager>
app.manage(topic_store);                 // Arc<std::sync::Mutex<TopicStore>> 或等价
app.manage(assistant);                   // Arc<AssistantMode>
app.manage(poller);                      // Arc<tokio::sync::Mutex<Poller>> 保持现状语义
app.manage(session_tracker);             // SessionTracker (Clone)
app.manage(providers);                   // ProviderRegistry 或 Arc
app.manage(engine);                      // Arc<Mutex<Engine>> 若 chat 需互斥
app.manage(GatewayMeta { ... });
// 过渡期可保留 Arc<Gateway> 仅作装配缓存，但命令不得持其锁跨 await
```

### 长路径伪代码

```rust
// bootstrap（setup）
let mgr = neuron_manager.clone(); // 已 manage，无需握整颗 Gateway
tauri::async_runtime::spawn(async move {
    let _ = mgr.bootstrap().await; // warning on err
});

// send_chat_message
async fn send_chat_message(...) {
    let (assistant, conv_id, model) = {
        // 短临界区：读 meta / clone assistant / 解析会话
        prepare_send(...)
    }; // drop 所有锁
    let response = assistant.converse(...).await?; // 网络在锁外
    {
        // 短临界区：写回 current_conversation_id / session
        finalize_send(...)
    }
    Ok(response)
}

// 读命令（例 list_topics）
async fn list_topics(topic_store: State<'_, Arc<Mutex<TopicStore>>>, ...) {
    let store = topic_store.lock().unwrap_or_async_equiv();
    store.list(...)
}
```

### 调用关系

```text
GUI invoke
  ├─ 读命令 → 域 State 短锁 → 返回
  ├─ send_chat / call_model → 短锁准备 → await 网络 → 短锁收尾
  └─ setup → clone NeuronManager → spawn bootstrap（锁外）

Poller（已有）→ 继续不经 Gateway 锁
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是——文档固化「分域 + 锁不住网络」单次交付。
- 若否，偏差在哪里：—
- 是否需要调整本轮目标或范围：已按用户要求合并原阶段 1+2。

## Checkpoint Summary

- 当前任务理解：一次做完 clone-out 长路径与 Tauri 分域 manage，消除无响应。
- 当前核心目标：锁不住网络；命令按域短锁；启动/对话不卡死窗口。
- 当前进度：**实现已落地**；`cargo test --lib` 67 passed；待用户手工确认窗口无「无响应」。
- 涉及文件 / 模块：`lib.rs`、`gateway.rs`、`engine.rs`、`architecture.md`。
- 风险：共享 SQLite 争用仍在 conn 层；手工冷启动验证依赖本地。
- 验证方式：`cargo test --lib`；冷启动拖窗口；Assistant 长调用时点 UI。
- Execution Approval: `Approved`

## Change Log

- 2026-08-01 12:07: 创建正式 spec；合并原计划阶段 1（止血）与阶段 2（分域）为单次交付；澄清 Promise.all 不堵 JS 渲染线程。
- 2026-08-01 12:24: 补充 Community Survey：Tauri 社区方案 A–E 对比；确认采纳 A+C，弃用 B 为默认，D Out，E 可选。
- 2026-08-01 12:35: 实现落地 — `Gateway`/`Engine` 长路径 `&self` + clone-out；Tauri 去掉外层 `Mutex<Gateway>`，分域 `manage`；读命令改 async；bootstrap spawn 只持 `NeuronManager`；`architecture.md` 写实现态；`cargo test --lib` 67 passed。

## Validation

- Self-check: 代码中无 `blocking_lock` / `Mutex<Gateway>` / `state.lock().await` 跨网络；bootstrap 与 `send_model_message` 均为 clone-out 后 await。
- Static checks: `cargo test --lib`（`CARGO_TARGET_DIR=.../src-tauri/target`）— **67 passed, 0 failed**。
- Runtime / Test: `pnpm tauri dev` 可冷启动并完成 bootstrap（见运行日志）；系统「无响应」手工验收待用户确认。
- Human confirmation: 待用户在长对话/bootstrap 期间拖窗口确认。
- 结果汇总: Done Contract 1–3 由代码+测试覆盖；契约第 4 条手工窗口响应待确认。
- 核心目标是否已由证据证明完成: **部分**（自动化证据齐；手工无响应确认未勾）。
- 若未完成，当前剩余差距: 请用户冷启动 + 发一条 Assistant 消息期间拖窗口/点 UI。
- 剩余风险: 极低（锁模型已拆）；若仍卡，查前端 `ready` 门闩或模型侧超时，非 Gateway 上帝锁。

## Resume / Handoff

- 当前状态: 实现完成，等待手工验收
- 当前卡点: 用户确认窗口无系统「无响应」弹窗
- 下一步唯一动作: 手工验收通过后可将 Validation Human confirmation 勾为通过
- 下一轮核心目标: （可选）emit 进度 E；前端 Loading UX
