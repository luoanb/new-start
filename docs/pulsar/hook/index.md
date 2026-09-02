# Hook 域结构架构（docs/pulsar/hook/index.md）

> 本目录只约束 Hook 域的**结构**：目录布局、类型契约、注册体系、依赖方向与扩展规则。
> 核对基准：2026-09-02 磁盘代码。行为细节（裁决语义、门控频率、账本字段）以
> `hook/` 各文件 doc-comment 与 spec（[2026-08-30 合并裁决](../../specs/2026-08-30_10-30_hook-gating-merged-judgement.md)、
> [2026-08-30 注册式重构](../../specs/2026-08-30_13-40_hook-registry-refactor.md)）为准。

## 1. 域定义与边界

Hook 域 = **核心五步之外的调度收拢层**：凡不属于轮次管线核心步骤（load_context → persist_input → call_model → execute_tools → persist_outcome）的调度（选型 / 课题路由 / 判定 / 打分 / 压缩 / 收尾复盘），一律以注入点 hook 形式挂载，由上层装配期注册，runner 只在注入点分发。

一句话：**runner 拥有流程，hook 拥有调度**。

- **本域负责**：注入点契约（挂在哪、能拿到什么、失败怎么办）、hook 注册与执行分发、裁决 hook 实例的定义内聚、裁决调用的纠偏与全量账本。
- **本域不负责**：轮次管线本身（Conversation 域）、裁决结果的业务消费语义主体（Assistant 域编排层）、被 hook 复用的能力实现（如 `Compactor` 属 Conversation 域）。

## 2. 目录结构（规范布局）

```text
core/hook/
  mod.rs          # 收拢层入口：模块声明 + 对外 re-export（唯一出口）
  defs.rs         # 注入点契约层：InjectPointId + HookHandler + HookDef + HookRegistry
  registry.rs     # 实例注册表（pub(crate)）：HookInstance + ACTIVE_HOOKS + LEGACY_HOOKS
  instances/      # 裁决实例：一个 hook 一个文件（见 §6）
    mod.rs          # 模块声明 + SYSTEM_TYPE_* 常量 re-export
    user_round_judgement.rs   # 启用（IP-1 合并裁决）
    round_review.rs           # 启用（IP-5 合并复盘）
    score_feedback.rs         # 休眠（legacy，代码与契约保留）
    match_topic.rs            # 休眠
    revise_topic.rs           # 休眠
    complete_scope.rs         # 休眠
  judgement.rs    # 裁决共享类型层：HookDef（规则表）+ 三态/明细/锚点 + hook_def() 查询
  store.rs        # 账本层：hook_judgements 表（两阶段写入，只读不删改）
  compaction.rs   # 基础设施 hook：把 Compactor 封装为 IP-2 hook 的注册函数
```

**归属约束**（什么放哪、不放哪）：

| 约束 | 依据 |
|---|---|
| 新增**裁决 hook** 只允许新增 `instances/<hook>.rs` 一个文件 + 清单登记一行 | [registry.rs:1-9](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/registry.rs#L1-L9) |
| `defs.rs` 是纯契约层，**禁止**引用本域其他模块或任何业务模块（只依赖 `RoundContext` / `ModelCallResponse` / `ToolResultItem` 等管线类型） | [defs.rs:19-24](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L19-L24) |
| `registry.rs` 为 `pub(crate)`：`HookRun` 签名引用 crate 内部类型 `AssistantHooks`，不对外泄露 | [mod.rs:21-22](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/mod.rs#L21-L22) |
| `judgement.rs` 只放共享类型与查询入口，**禁止**放具体 hook 的定义（定义内聚在实例文件） | [judgement.rs:1-8](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/judgement.rs#L1-L8) |
| `store.rs` 与 `topic_store` 同构（`conn: Arc<Mutex<Connection>>` + `on_change` + `init_table` + 统一 `emit_change`），只读账本：无更新/删除/重跑命令 | [store.rs:1-8](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/store.rs#L1-L8) |
| 非裁决类基础设施 hook（如压缩）写成 `register(registry, …)` 装配函数放独立文件，**不进** `instances/`（instances 专指裁决实例） | [compaction.rs:1-9](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/compaction.rs#L1-L9) |
| `SYSTEM_TYPE_SELECT_NEURON`（选型器）非裁决 hook，**不收拢**进本域，常量留在 `assistant_session.rs` | [judgement.rs:9-10](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/judgement.rs#L9-L10) |

## 3. 核心类型契约：两套 `HookDef` 同名不同物

结构上最易混淆的点，**按此约定引用**：

| | `defs::HookDef`（注册单元） | `judgement::HookDef`（裁决规则表） |
|---|---|---|
| 标识 | `id`（如 `assistant.round.before`） | `system_type`（如 `assistant_user_round_judgement`） |
| 挂载 | `inject_point: InjectPointId`（类型化枚举） | `inject_point: &'static str`（`as_str()` 字符串，供清单过滤与账本列） |
| 携带 | `handler: HookHandler`（可执行的闭包变体） | `response_format`（结构化输出契约）+ `neutral_fallback`（中性降级值） |
| 生命周期 | 装配期动态注册进 `HookRegistry` | 编译期静态清单（`ACTIVE_HOOKS` / `LEGACY_HOOKS`） |
| 消费者 | runner 在注入点分发 | `hook_def()` / `hook_defs_meta()` / 裁决调用 `call_judgement` |

引用约定（[mod.rs:14-15](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/mod.rs#L14-L15)）：`defs::HookDef` 按全路径或别名 `HookRegistration` 引用；`judgement::HookDef` re-export 到 `hook::` 顶层并占用 `HookDef` 名字（既有消费者不变）。

`HookHandler` 五个变体与 `InjectPointId` 一一对应（[defs.rs:60-83](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L60-L83)）：第一参恒为当前轮完整 `RoundContext`（`&mut` 可改 / `&` 只读），IP-3/IP-4 追加就近局部产物（`ModelCallResponse` / `Vec<ToolResultItem>`）作第二 `&mut` 参数。设计原则是**注入点即类型**：无独立 kind 分类，挂在哪决定能消费什么、能做什么；是否执行由 handler 内部自行判断（无 guard 机制，mandatory/guard 已移除）。

## 4. 双注册体系（结构核心）

Hook 域存在**两套注册机制，各管一层**，不得混用：

| | 动态注册表 `defs::HookRegistry` | 静态清单 `registry::ACTIVE_HOOKS` / `LEGACY_HOOKS` |
|---|---|---|
| 存什么 | 装配期注册的全部注入点 hook（业务壳 + 选型 + 压缩） | 裁决实例（`HookInstance = judgement::HookDef + HookRun`） |
| 注册方式 | 运行时 `register(HookDef)`，同 id 重复拒绝、卸载按 id | 改代码：实例引用在两清单间移动（回切 = 一行） |
| 分发者 | runner 在 5 个注入点调用 `run_*` | 业务 hook 内部经 `active_hooks_at(point)` 遍历 |
| 当前成员 | `assistant.round.before`（IP-1）、`assistant.round.after`（IP-5）、`assistant.select-neuron`（IP-1）、`core.compaction`（IP-2） | 启用 2：`user_round_judgement`（IP-1）、`round_review`（IP-5）；休眠 4：`score_feedback` / `match_topic` / `revise_topic` / `complete_scope` |
| 定义位置 | [defs.rs:136](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L136) / 装配见 [gateway.rs:377-459](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L377-L459) | [registry.rs:38-51](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/registry.rs#L38-L51) |

两级分发链路（以 IP-1 为例）：

```text
ConversationRunner::run_round
  └─ hooks.run_after_load_context(ctx, on_session_switch)      ← 注入点分发（动态注册表）
       └─ "assistant.round.before" handler
            └─ AssistantHooks::round_before(ctx)               ← 业务编排（模式门控/解析课题）
                 └─ active_hooks_at(AfterLoadContext)          ← 静态清单分发
                      └─ user_round_judgement::run(...)        ← 裁决实例（门控下沉在 run 内）
```

约束：编排层（`round_before` / `round_after`）保持**中立**——只做模式门控与公共前置（resolve / release / advance_brief / 计数），**禁止按 `system_type` 特判**；实例自带门控（[registry-refactor spec](../../specs/2026-08-30_13-40_hook-registry-refactor.md)），保证 legacy 实例回切时语义忠实。

## 5. 注入点与失败策略

| 注入点 | 挂载位置 | handler 权限 | 失败策略 | 当前注册者 |
|---|---|---|---|---|
| IP-1 `after_load_context` | ① load_context 后 | `&mut RoundContext`，可改写 `session_id` 触发会话切换 | **fail**（Err 上抛中止本轮） | `assistant.round.before`、`assistant.select-neuron` |
| IP-2 `after_persist_input` | ② persist_input 后 | `&mut RoundContext`（改 wire 只影响本次发送，不动真相源） | ignore | `core.compaction` |
| IP-3 `after_call_model` | ③ call_model 后 | `&mut RoundContext` + `&mut ModelCallResponse` | ignore | （无注册者） |
| IP-4 `after_execute_tools` | ④ execute_tools 后 | `&mut RoundContext` + `&mut Vec<ToolResultItem>` | ignore | （无注册者） |
| IP-5 `after_persist_outcome` | ⑤ persist_outcome 后 | `&RoundContext` 只读，副作用自办 | ignore | `assistant.round.after` |

结构约束（[defs.rs:7-11](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L7-L11)）：

- **失败策略梯度**：越靠前越硬、越靠后越软——数据一旦入库（persist_input 后），中止会丢轮次产物；故只有 IP-1 fail，其余 ignore（warn 日志后按原值继续）。
- **执行顺序** = 组内注册顺序，后注册者看到先注册者的修改，可继续改写（链式 `&mut`）。
- **并发约束**：`HookRegistry` 内部 `Mutex`；执行前锁内取 Arc 快照、**锁外 await**（不跨 await 持锁）（[defs.rs:130-135, 199-206](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L130-L135)）。
- **IP-1 会话切换**：hook 改写 `ctx.session_id` 后，runner 经 `on_session_switch` 回调 reload 新会话，后续 hooks 基于最终会话数据执行（[defs.rs:208-230](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/defs.rs#L208-L230)）。
- **IP-1 组内顺序敏感**：装配方必须先注册课题路由（`assistant.round.before`）再注册选型（`assistant.select-neuron`）——选型需基于路由后的最终会话（[gateway.rs:405-410](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/gateway.rs#L405-L410)）。
- **边界只画当前轮**：不跨会话、不跨轮、不给全局状态。

## 6. 裁决实例结构（一个 hook 一个文件）

每个 `instances/<hook>.rs` 必须内聚**五件套**（模板见 [user_round_judgement.rs](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/instances/user_round_judgement.rs#L24-L79)）：

1. `SYSTEM_TYPE_<NAME>` 常量（账本 `hook_type` 列的值；`instances/mod.rs` 统一 re-export）；
2. `<NAME>_SCHEMA`：strict JSON Schema——顶层 `additionalProperties: false`、全字段 `required`，可选用 `["T","null"]` 联合表达可选（测试 [judgement.rs:160-180](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/hook/judgement.rs#L160-L180) 保证）；
3. `fallback_<name>()`：**中性**降级默认值（裁决失败时主轮次不中断，如 score=0 / 空 diff）；
4. `INSTANCE: HookInstance`（def + run 的静态注册单元）；
5. `run(hooks, ctx)`：执行逻辑，**门控写在 run 内**（如未绑定课题必跑、收尾轮才复盘），经 `run_boxed` 适配为 `HookRun::Before/After` fn 指针。

裁决调用的纠偏与落库**不进实例文件**，统一收敛在 `AssistantSession::call_judgement`（[assistant_session.rs:285](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/assistant_session.rs#L285)）：

- **C 结构化输出预防**：`def.response_format` 经能力探测降级链下发（json_schema → json_object → 无约束；探测结果按 `(provider_id, model_id, config_generation)` 进程内缓存）；
- **B 有限重试**：首轮解析失败 → payload 追加 `_feedback`（原输出 + 「仅返回 JSON」指令）重试 1 次；
- **A 中性降级**：重试仍失败 → 返回 `Downgraded`（`def.neutral_fallback()`），不再上抛；
- **全链路账本**：`insert_start`（pending + 锚点事件）→ `finish`（终态 + 事件）两阶段写入 `hook_judgements` 表；终态三值 `ok / retried_ok / downgraded`，`attempts_detail` 全量保留原文。

**模型同源约束**：裁决 hook 与主对话共用 `ctx.model`（用户所选），禁止读配置默认模型（[micro-spec](../../micro_specs/2026-08-14_16-45_hook-model-same-source.md)）。

## 7. 依赖方向

### 7.1 域内

```text
defs.rs ──（自洽，不依赖域内其他模块）
registry.rs ⇄ judgement.rs        # registry 用 judgement::HookDef；judgement 查 registry::ACTIVE_HOOKS
instances/* → { defs, judgement, registry }   # 实例消费契约与清单类型
store.rs ──（独立账本，仅依赖 error/events）
compaction.rs → defs             # 基础设施 hook 只依赖契约层
```

`registry ⇄ judgement` 与 `instances ⇄ assistant_session` 是**同 crate 内有意保留的模块环**（Rust 合法）：实例的执行签名 `HookRun` 以 `AssistantHooks<'_>`（业务执行上下文，编排层构造并传入）为参数——实例不持有业务单例，业务能力一律经该参数访问；`registry.rs` 因此收为 `pub(crate)`。

### 7.2 域外

| 方向 | 关系 |
|---|---|
| Conversation 域（runner）→ Hook 域 | 持有 `Arc<HookRegistry>`，仅在 5 个注入点调用 `run_*`（[conversation_runner.rs:177-258](file:///home/lab/Documents/trae_projects/new-start-wt/packages/pulsar-app/src-tauri/src/core/conversation_runner.rs#L177-L258)）；runner 不感知任何具体 hook |
| Gateway → Hook 域 | 装配容器：创建注册表、按顺序注册业务/选型/压缩 hook、持有账本 store |
| Hook 域（instances）→ Assistant 域 | 只依赖 `AssistantHooks` 上下文类型与其 `pub(crate)` 帮助函数（门控谓词、scope 构造等） |
| 入口层（lib.rs / net/rpc.rs）→ Hook 域 | 只经 re-export 消费：`hook_defs_list` / `hook_judgements_list` 两个查询命令 |
| 前端 → Hook 域 | 只读：`HookDefMeta`（面板下拉）+ `StateChange::HookJudgements` 锚点事件（裁决卡就地渲染 pending→终态），不感知 Rust 静态表 |

## 8. 扩展规则（改什么、不许改什么）

| 场景 | 允许的动作 | 禁止 |
|---|---|---|
| 新增裁决 hook | 新建 `instances/<name>.rs`（五件套）+ 加入 `ACTIVE_HOOKS` 或 `LEGACY_HOOKS` 一行 | 改 `judgement.rs` / 编排层 / 既有实例 |
| 回切 legacy hook | 实例引用从 `LEGACY_HOOKS` 移入 `ACTIVE_HOOKS`（一行）；inserts 契约与神经元种子已在原位 | 重写门控或 run 逻辑（原语义每轮跑） |
| 下线 hook | 移入 `LEGACY_HOOKS`（惰性弃用：代码、schema、inserts 全保留，不执行、不进面板） | 删文件、删 inserts、删测试 |
| 新增业务/基础设施 hook | 装配期（gateway 或独立 register 函数）注册进 `HookRegistry`；id 全局唯一；`Weak` 捕获会话防循环引用 | 在 runner 核心五步里写业务 if/else |
| 新增注入点 | `InjectPointId` 加变体 + `HookHandler` 加变体 + `run_*` 方法，按梯度定失败策略 | 跳过 handler 变体直接用统一签名 |
| 改 `defs.rs` | 仅扩展契约 | 引入域内/业务依赖，破坏其自洽性 |

不变量（测试锁定）：`ACTIVE_HOOKS` 恰 2 条、`LEGACY_HOOKS` 恰 4 条、`system_type` 全局唯一、`HookRun` 变体与 `inject_point` 一一对应、每实例必带 strict schema 与中性 fallback。

## 9. 快速索引

- 启用实例作用约定 → [user-round-judgement.md](./user-round-judgement.md)（用户轮裁决）· [round-review.md](./round-review.md)（轮次复盘）
- 合并裁决的门控与契约 → [specs/2026-08-30_10-30_hook-gating-merged-judgement.md](../../specs/2026-08-30_10-30_hook-gating-merged-judgement.md)
- 注册式重构（instances / 双清单）→ [specs/2026-08-30_13-40_hook-registry-refactor.md](../../specs/2026-08-30_13-40_hook-registry-refactor.md)
- 模型同源 → [micro_specs/2026-08-14_16-45_hook-model-same-source.md](../../micro_specs/2026-08-14_16-45_hook-model-same-source.md)
- 域在整体架构中的位置 → [../architecture.md](../architecture.md)（Hook 域小节 + IP 流程时序图）
