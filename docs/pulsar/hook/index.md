# Hook 域结构架构（docs/pulsar/hook/index.md）

> 本目录只约束 Hook 域的**结构**：目录布局、定义/注册/开启三阶段、类型契约、依赖方向与扩展规则。
> 核对基准：2026-09-12 磁盘代码。行为细节（裁决语义、门控频率、账本字段）以
> `hook/` 各文件 doc-comment 与 spec 为准。

## 1. 域定义与边界

Hook 域 = **核心五步之外的调度收拢层**：凡不属于轮次管线核心步骤（load_context → persist_input → call_model → execute_tools → persist_outcome）的调度（选型 / 课题路由 / 判定 / 打分 / 压缩 / 收尾复盘），一律以注入点 hook 形式挂载，由上层装配期注册，runner 只在注入点分发。

一句话：**runner 拥有流程，hook 拥有调度**。

- **本域负责**：注入点契约（挂在哪、能拿到什么、失败怎么办）、hook 的定义 / 注册 / 开启、注册与执行分发、裁决定义内聚、裁决调用的纠偏与全量账本。
- **本域不负责**：轮次管线本身（Conversation 域）、裁决结果的业务消费语义主体（Assistant 域编排层）、被 hook 复用的能力实现（如 `Compactor` 属 Conversation 域）。

## 2. 目录结构（规范布局）

```text
core/hook/                      # 插槽协议（封闭核心）
  mod.rs          # 协议出口：re-export InjectPointId / HookHandler / HookRegistry / RegisterError（不导出 HookDef，避免重名）
  defs.rs         # 注入点契约 + 定义/注册/开启：InjectPointId / HookHandler / HookDef（定义）/ HookRegistry（注册 + 开启 + 分发）

application/hook/               # 裁决业务（应用侧）
  mod.rs          # 业务出口：定义元数据 + 账本 + 压缩装配
  instances/      # 裁决定义：一个 hook 一个文件
    mod.rs
    user_round_judgement.rs   # 装配中（IP-1 合并裁决）：SPEC + register + run
    round_review.rs           # 装配中（IP-5 合并复盘）：SPEC + register + run
    score_feedback.rs         # 休眠（**定义·未注册**）
    match_topic.rs            # 休眠
    revise_topic.rs           # 休眠
    complete_scope.rs         # 休眠
  judgement.rs    # 裁决定义元数据层：JudgementSpec + JUDGEMENT_SPECS + judgement_spec()/hook_defs_meta() + 裁决结果类型
  store.rs        # 账本层：hook_judgements 表（两阶段写入，只读不删改）
  compaction.rs   # 基础设施 hook：把 Compactor 封装为 IP-2 hook 的装配函数
```

**归属约束**（什么放哪、不放哪）：

| 约束 | 依据 |
|---|---|
| 新增**裁决**只允许新增 `instances/<hook>.rs` 一个文件 + 在定义清单登记一行 + 在 `install_hooks` 加一次 `register` | [instances/mod.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/application/hook/instances/mod.rs) |
| `defs.rs` 是纯契约层，**禁止**引用本域其他模块或任何业务模块（只依赖 `RoundContext` / `ModelResponse` / `ToolResult` 等管线类型） | [defs.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/core/hook/defs.rs) |
| `judgement.rs` 只放**定义元数据与共享类型**（`JudgementSpec` / `JUDGEMENT_SPECS` / 查询入口 / 裁决结果类型），**禁止**放具体 hook 的 run 实现 | [judgement.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/application/hook/judgement.rs) |
| `store.rs` 与 `topic_store` 同构（`conn: Arc<Mutex<Connection>>` + `on_change` + `init_table` + 统一 `emit_change`），只读账本：无更新/删除/重跑命令 | [store.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/application/hook/store.rs) |
| 非裁决类基础设施 hook（如压缩）写成 `register(registry, …)` 装配函数放独立文件，**不进** `instances/`（instances 专指裁决） | [compaction.rs](file:///home/lab/Documents/trae_projects/new-start/packages/pulsar-app/src-tauri/src/application/hook/compaction.rs) |
| `SYSTEM_TYPE_SELECT_NEURON`（选型器）非裁决，**不收拢**进本域，常量留在 `assistant_session.rs` | 同左 |

## 3. 定义 / 注册 / 开启（三阶段分离）

三者**正交**，不得混为一谈：

| 阶段 | 含义 | 载体 | 不做会怎样 |
|---|---|---|---|
| **定义** Definition | hook 的契约 + 元数据 + 实现，作为代码恒在 | 核心 `HookDef`（契约 + 实现）；裁决另有 `JudgementSpec`（定义元数据） | —— |
| **注册** Registration | 把定义登记进注册表，系统知道它挂哪个注入点、可分发给它 | `HookRegistry::register(def)`（**默认关闭**） | 系统不认它（代码仍在） |
| **开启** Enablement | 注册条目是否真正执行 | 条目 `enabled` + `set_enabled(id, on)`（运行时可切） | 在册、挂在注入点，但分发时跳过 |

合法组合：**定义·未注册** / **注册·未开启** / **注册·已开启**。

- 休眠的旧 4 条裁决 = **定义·未注册**（源码保留，不进 `JUDGEMENT_SPECS`、不注册、不开启）。
- 装配中的两条裁决与业务 / 选型 / 压缩 hook = **注册·已开启**（装配期 `register` 后显式 `set_enabled(id, true)`）。

**只有一套注册表**：裁决不再有独立的 `ACTIVE_HOOKS` / `HookInstance` / `HookRun`，也不再有壳 hook 手写 `for + match` 二次分发（旧「缝合」已删除）。裁决的 handler 就是核心 `HookHandler` 闭包，装配期直接 `register` 进 `HookRegistry`。

## 4. 注入点与失败策略

| 注入点 | 挂载位置 | handler 权限 | 失败策略 | 当前注册者（已开启） |
|---|---|---|---|---|
| IP-1 `after_load_context` | ① load_context 后 | `&mut RoundContext`，可改写 `session_id` 触发会话切换 | **fail**（Err 上抛中止本轮） | `assistant.round.before`、`assistant.user-round-judgement`、`assistant.select-neuron` |
| IP-2 `after_persist_input` | ② persist_input 后 | `&mut RoundContext`（改 wire 只影响本次发送，不动真相源） | ignore | `core.compaction` |
| IP-3 `after_call_model` | ③ call_model 后 | `&mut RoundContext` + `&mut ModelResponse` | ignore | （无注册者） |
| IP-4 `after_execute_tools` | ④ execute_tools 后 | `&mut RoundContext` + `&mut Vec<ToolResult>` | ignore | （无注册者） |
| IP-5 `after_persist_outcome` | ⑤ persist_outcome 后 | `&RoundContext` 只读，副作用自办 | ignore | `assistant.round-review`、`assistant.round.after` |

结构约束：

- **失败策略梯度**：越靠前越硬、越靠后越软——数据一旦入库（persist_input 后），中止会丢轮次产物；故只有 IP-1 fail，其余 ignore（warn 日志后按原值继续）。
- **执行顺序** = 组内注册顺序（只跑已开启条目），后注册者看到先注册者的修改，可继续改写（链式 `&mut`）。
- **注册序即相位序**（`install_hooks` / gateway 装配期保证）：
  - IP-1：`assistant.round.before`（模式门控 / 课题解析 / 简报推进）→ `assistant.user-round-judgement` → `assistant.select-neuron`（gateway 随后注册）。
  - IP-5：`assistant.round-review`（复盘）→ `assistant.round.after`（计数）。
- **并发约束**：`HookRegistry` 内部 `Mutex`；执行前锁内取 Arc 快照（只取 enabled）、**锁外 await**（不跨 await 持锁）。
- **IP-1 会话切换**：hook 改写 `ctx.session_id` 后，runner 经 `on_session_switch` 回调 reload 新会话，后续 hooks 基于最终会话数据执行。
- **边界只画当前轮**：不跨会话、不跨轮、不给全局状态。

## 5. 裁决定义结构（一个 hook 一个文件）

每个 `instances/<hook>.rs` 内聚：

1. `SYSTEM_TYPE_<NAME>` 常量（账本 `hook_type` 列的值；`instances/mod.rs` 统一 re-export）；
2. `<NAME>_SCHEMA`：strict JSON Schema——顶层 `additionalProperties: false`、全字段 `required`，可选用 `["T","null"]` 联合表达可选；
3. `fallback_<name>()`：**中性**降级默认值（裁决失败时主轮次不中断，如 score=0 / 空 diff）；
4. `SPEC: JudgementSpec`（**定义**：system_type / label / inject_point / response_format / neutral_fallback）；
5. `run(hooks, ctx)`：执行逻辑，**门控写在 run 内**（模式 / 触发类型 / 未绑定课题 / 收尾轮 `is_settling_round` 等）；
6. 装配中的裁决另加 `register(registry, &Arc<AssistantSession>)`：把定义包成核心 `HookDef` 闭包（捕获 `Weak<AssistantSession>` + `SPEC`）注册进 `HookRegistry`。

裁决调用的纠偏与落库**不进实例文件**，统一收敛在 `AssistantSession::call_judgement`：

- **C 结构化输出预防**：`spec.response_format` 经能力探测降级链下发（json_schema → json_object → 无约束；探测结果按 `(provider_id, model_id, config_generation)` 进程内缓存）；
- **B 有限重试**：首轮解析失败 → payload 追加 `_feedback`（原输出 + 「仅返回 JSON」指令）重试 1 次；
- **A 中性降级**：重试仍失败 → 返回 `Downgraded`（`spec.neutral_fallback()`），不再上抛；
- **全链路账本**：`insert_start`（pending + 锚点事件）→ `finish`（终态 + 事件）两阶段写入 `hook_judgements` 表；终态三值 `ok / retried_ok / downgraded`，`attempts_detail` 全量保留原文。

**模型同源约束**：裁决与主对话共用 `ctx.model`（用户所选），禁止读配置默认模型。

## 6. 依赖方向

### 6.1 域内

```text
core/hook/defs.rs ──（自洽，不依赖应用侧与业务）
application/hook/instances/* → { core::hook::defs, judgement }   # 定义消费契约与元数据类型
application/hook/judgement.rs → instances::*::SPEC               # 定义清单汇聚（同 crate 模块环，Rust 合法）
application/hook/compaction.rs → core::hook::defs               # 基础设施 hook 只依赖契约层
application/hook/store.rs ──（独立账本，仅依赖 error / events）
```

## 7. 域外

| 方向 | 关系 |
|---|---|
| Conversation 域（runner）→ Hook 域 | 持有 `Arc<HookRegistry>`，仅在 5 个注入点调用 `run_*`；runner 不感知任何具体 hook |
| Gateway / assistant_session → Hook 域 | 装配容器：创建注册表，注册业务 / 选型 / 压缩 / 裁决 hook，并逐条 `set_enabled` 开启 |
| Hook 域（instances）→ Assistant 域 | 只依赖 `AssistantHooks` 上下文类型与其 `pub(crate)` 帮助函数（门控谓词、scope 构造等） |
| 入口层（lib.rs / net/rpc.rs）→ Hook 域 | 只经 re-export 消费：`hook_defs_list` / `hook_judgements_list` 两个查询命令 |
| 前端 → Hook 域 | 只读：`HookDefMeta`（面板下拉）+ `StateChange::HookJudgements` 锚点事件（裁决卡就地渲染 pending→终态） |

## 8. 扩展规则（改什么、不许改什么）

| 场景 | 允许的动作 | 禁止 |
|---|---|---|
| 新增裁决 | 新建 `instances/<name>.rs`（常量 + schema + fallback + SPEC + run + register）+ 定义清单登记一行 + `install_hooks` 注册并开启 | 改 `judgement.rs` 的共享类型 / 编排层 / 既有实例 |
| 启用休眠裁决 | 为其补 `register` 装配入口，并在 `install_hooks` 注册 + `set_enabled` 开启 | 重写门控或 run 逻辑（原语义每轮跑） |
| 下线裁决 | 从装配处移除 `register` / 改为不 `set_enabled`（惰性弃用：代码、schema、inserts 全保留） | 删文件、删 inserts、删测试 |
| 新增业务 / 基础设施 hook | 装配期注册进 `HookRegistry` 并 `set_enabled`；id 全局唯一；`Weak` 捕获会话防循环引用 | 在 runner 核心五步里写业务 if/else |
| 运行期启停 | `set_enabled(id, on)`（注册条目仍在册，可再次开启） | 把启停做成编译期清单成员关系 |
| 新增注入点 | `InjectPointId` 加变体 + `HookHandler` 加变体 + `run_*` 方法，按梯度定失败策略 | 跳过 handler 变体直接用统一签名 |
| 改 `defs.rs` | 仅扩展契约 | 引入域内 / 业务依赖，破坏其自洽性 |

不变量（测试锁定）：`register` 默认关闭；注册未开启不分发；`set_enabled` 可运行期开关；`set_enabled` 未知 id 报错；每实例必带 strict schema 与中性 fallback。

## 9. 快速索引

- 启用实例作用约定 → [user-round-judgement.md](./user-round-judgement.md)（用户轮裁决）· [round-review.md](./round-review.md)（轮次复盘）
- 三阶段分离（定义 / 注册 / 开启）→ [specs/2026-09-12_10-42_hook-definition-registration-enablement.md](../../specs/2026-09-12_10-42_hook-definition-registration-enablement.md)
- 合并裁决的门控与契约 → [specs/2026-08-30_10-30_hook-gating-merged-judgement.md](../../specs/2026-08-30_10-30_hook-gating-merged-judgement.md)
- 注册式重构（历史：instances / 双清单）→ [specs/2026-08-30_13-40_hook-registry-refactor.md](../../specs/2026-08-30_13-40_hook-registry-refactor.md)
- 模型同源 → [micro_specs/2026-08-14_16-45_hook-model-same-source.md](../../micro_specs/2026-08-14_16-45_hook-model-same-source.md)
- 域在整体架构中的位置 → [../architecture.md](../architecture.md)（Hook 域小节 + IP 流程时序图）
