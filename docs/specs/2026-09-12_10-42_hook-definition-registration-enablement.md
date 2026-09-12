# Hook 三阶段分离：定义 / 注册 / 开启

> 2026-09-12 · standard · 方向已获用户确认（对话内：定义/注册/开启三分离；注册默认关、初始化显式开启、运行时可切换）。**已落地**（见文末「落地结果」）。

## 复述理解

现状把 hook 的三件事揉在了一起：

- **定义**：`judgement::HookDef` + `run` 实现（业务裁决定义）。
- **注册 + 开启**：`ACTIVE_HOOKS`——进数组即"既注册又开启"，唯一的关闭方式是移出数组（移出即注销）。
- **定义未注册**：`LEGACY_HOOKS`。

且裁决实例从未接入核心 `HookRegistry`，只能靠壳 hook 手写 `for + match` 二次分发（"缝合"，见 §现状问题）。

用户要求三者**正交**：

- 可以**定义着不注册**；
- 可以**注册了不开启**；
- **注册默认关**，初始化时设置需要开启的项；
- **运行时可切换**。

## 三阶段模型

| 阶段 | 含义 | 载体 | 不做的后果 |
|---|---|---|---|
| 定义 Definition | hook 的契约 + 元数据 + 实现，作为代码恒在 | 核心 `HookDef`（契约+实现）；业务 `JudgementSpec`（元数据） | —— |
| 注册 Registration | 把定义登记进注册表，系统知道它挂哪个注入点、可分发给它 | `HookRegistry::register(def)` | 系统不认它（代码仍在） |
| 开启 Enablement | 注册条目是否真正执行 | 注册条目 `enabled` 状态 + `set_enabled(id, on)` | 在册、可分发给它，但分发时跳过 |

合法组合：定义·未注册 / 注册·未开启 / 注册·已开启。

## 契约变化（core/hook/defs.rs）

```rust
struct RegisteredHook {
    def: Arc<HookDef>,
    enabled: bool, // 在既有 Mutex 下，无需原子类型
}

impl HookRegistry {
    /// 注册（默认关闭；开启由调用方显式 set_enabled）。
    pub fn register(&self, def: HookDef) -> Result<(), RegisterError>;

    /// 开启 / 关闭已注册条目（运行时可调）。
    pub fn set_enabled(&self, id: &str, on: bool) -> Result<(), RegisterError>;

    pub fn is_enabled(&self, id: &str) -> bool;
    pub fn is_registered(&self, id: &str) -> bool;

    // run_after_* 分发：snapshot 只取 enabled 条目（其余全部行为不变）。
}
```

- `RegisterError` 增加未知 id 变体（`set_enabled` 目标不存在时）。
- **`register` 不再隐式开启**：全部注册点（业务壳 / 选型 / 压缩 / 裁决）都改为"注册 + 初始化显式开启"。
- 契约其余部分（`InjectPointId` / `HookHandler` / 五注入点分发 / 失败策略 / IP-1 会话切换 reload）**不变**。

## 应用层变化

### 裁决定义（application/hook/judgement.rs）

- `HookDef` → **`JudgementSpec`**（消除与 `defs::HookDef` 的同名冲突）：字段不变
  `{ system_type, label, inject_point, response_format, neutral_fallback }`。
- `hook_def(system_type)` → `judgement_spec(system_type)`；定义清单 `JUDGEMENT_SPECS`（纯数据，声明"有哪些裁决定义"，**不代表注册、不代表开启**）。
- `JudgementStatus` / `JudgementOutcome` / `JudgementAnchor` / `AttemptRecord` / `HookDefMeta` / 账本：不变。

### 裁决实例（application/hook/instances/*.rs）

- 每个文件暴露：`SPEC: &'static JudgementSpec` + `run(hooks, ctx)` 实现（**定义**）。
- 新增装配入口 `register(registry, &Arc<AssistantSession>)`：构造核心 `HookDef`（`id = spec.system_type`，`inject_point` 取 spec）注册；handler 是**核心闭包**，捕获 `Weak<AssistantSession>` + `SPEC`。
  - → 裁决**直接进核心注册表**，壳内 `for + match` 二次分发（缝合线）消失。
- 门控下沉到 `run` 内：`mode ∈ {Assistant, System}`、`trigger == User`（IP-1）自检（与既有 `need_user_round_judgement` / `is_settling_round` 同构）。

### 编排（application/assistant_session.rs）

- `install_hooks` 改为注册 4 个核心闭包（注册序即执行序）：
  - IP-1：`assistant.round.before`（模式门控 + `resolve_bound_topic` + User 时 `release_waiting_user` / Manual·Poller 时 `advance_brief`）
  - IP-1：`assistant.user-round-judgement`（裁决定义）
  - IP-5：`assistant.round-review`（裁决定义）
  - IP-5：`assistant.round.after`（仅 `tick_round_counters`）
  - 注册后对以上 4 项 + gateway 的 `assistant.select-neuron` + `core.compaction` 显式 `set_enabled(.., true)`。
- `round_before` / `round_after` 去掉 `active_hooks_at` 循环；IP-5 顺序保证 review 在 tick 之前。

### 删除项

- `application/hook/registry.rs`：`HookInstance` / `HookRun` / `ACTIVE_HOOKS` / `LEGACY_HOOKS` / `active_hooks_at`（及其测试）。
- 壳内 `for instance in active_hooks_at(..)` 二次分发与 `HookRun::Before/After` 的 `unreachable!` 配对断言。

## 不改动面

- `hook_defs_list` 出参语义：返回**已注册且已开启**的裁决（现状 2 条）。
- 账本 `hook_judgements` 表结构、`HookJudgementRecord` 字段。
- `call_judgement` 的 A/B/C 纠偏、两阶段落库、模型同源（仅参数类型 `HookDef` → `JudgementSpec`）。
- `core/round_service.rs` 五注入点分发、`run_raw_round`、`policies/`、`stores/`。

## 范围决策（无异议即按默认执行）

1. **休眠 4 条（`score_feedback` / `match_topic` / `revise_topic` / `complete_scope`）**：
   **默认 A**——保留源码为"定义·未注册"（不注册、不开启、不进面板），行为与现状一致；
   备选 B——注册为"注册·不开启"以便面板可见，但语义已被合并裁决取代，误开启会双跑，需额外"休眠"标记，暂不推荐。
2. **运行时切换的对外入口**：
   **默认**——本轮只做核心 + 应用层能力（`set_enabled` + 查询），面板/wire 若需暴露再作为独立迭代（要动前端与命令出参）。
   若要求本轮就可在界面切换，需追加：`hook_defs_list` 出参加 `enabled` + 新增切换命令 + 前端，请提前说明。

## 落地步骤（每步可编译）

1. `core/hook/defs.rs`：`RegisteredHook.enabled` + `register` 默认关 + `set_enabled`/`is_enabled`/`is_registered`；分发只跑 enabled。
2. 全部注册点补"注册 + 显式开启"（gateway 的选型/压缩、`install_hooks` 4 项）。
3. `judgement::HookDef` → `JudgementSpec`（改名 + 定义清单 + 查询入口）；`call_judgement` 换参。
4. 裁决实例改 `SPEC` + `register` 装配入口 + 门控下沉；删除 `registry.rs` 与壳内二次分发。
5. 清理 `hook/mod.rs` re-export、gateway、lib/rpc 引用；更新测试。
6. 反向同步文档：`docs/pulsar/hook/index.md`（当前已滞后，含目录布局）与 `docs/pulsar/architecture.md` Hook 域。

## Done Contract

1. `cargo check --all-targets` 0 错 0 警告；`cargo test --all-targets` 全绿；四入口二进制编译通过。
2. 全仓无 `HookInstance` / `HookRun` / `ACTIVE_HOOKS` / `LEGACY_HOOKS` / `active_hooks_at` 残留；`application/hook/registry.rs` 已删。
3. 测试锁定三阶段：
   - 注册后未开启 → 注入点分发**不执行**该 hook；
   - `set_enabled(id, true)` 后执行；
   - `set_enabled(id, false)` 后停止；
   - 未注册 id 的 `set_enabled` 返回错误。
4. 测试锁定注册序：IP-1 中 judgement 在 `select-neuron` 之前；IP-5 中 review 在 tick 之前。
5. 回归：助手模式裁决/复盘行为、账本写入、`hook_defs_list` 出参与现状一致。

## 风险

- **注册序敏感**从静态清单转移到 `install_hooks` 内的注册顺序 → 靠注释 + 测试锁定。
- 裁决闭包对所有模式都会拿到 `RoundContext` → 门控必须自检（`mode` / `trigger`），漏检会在 Chat/Agent 轮误触发 → 用测试覆盖。
- `register` 默认关是全量行为变更 → 漏了某处 `set_enabled` 会导致该 hook 静默不执行 → 用"四入口冒烟 + 既有 hooks 回归"兜底。

## 落地结果（2026-09-12，已完成）

**已落地**：

- `core/hook/defs.rs`：`RegisteredHook.enabled`；`register` 默认关闭；新增 `set_enabled` / `is_enabled` / `is_registered`；`snapshot` 只取 enabled 条目；`RegisterError` 增 `UnknownId`。
- 全部注册点改「注册 + 初始化显式开启」：`AssistantSession::install_hooks` 注册 4 条（`assistant.round.before` / `assistant.user-round-judgement` / `assistant.round-review` / `assistant.round.after`）并逐条开启；gateway 的选型 `assistant.select-neuron` 与 `core.compaction` 同样注册后开启。
- `judgement::HookDef` → **`JudgementSpec`**（同名冲突消除）；`JUDGEMENT_SPECS` 定义清单（2 条装配中）；`judgement_spec()` / `hook_defs_meta()`；`call_judgement` / `format_for_support` 换参。
- 裁决实例：`SPEC` + `register(registry, &Arc<AssistantSession>)` 装配入口（核心闭包，捕获 `Weak<AssistantSession>` + `SPEC`）；门控下沉（`mode ∈ {Assistant, System}`、IP-1 `trigger == User`）至 `run` 内。
- **删除**：`application/hook/registry.rs`（`HookInstance` / `HookRun` / `ACTIVE_HOOKS` / `LEGACY_HOOKS` / `active_hooks_at`）；壳内 `for + match` 二次分发；壳 hook 与 `AssistantHooks` 的裁剪（`round_before` 只做门控 / 解析 / 简报；`round_after` 只做计数）。
- 休眠 4 条：改为 **定义·未注册**（保留源码 `SPEC` + `run`，不进定义清单、不注册、不开启）。
- 反向同步：`docs/pulsar/hook/index.md`（重写为三阶段结构）、`docs/pulsar/architecture.md`（Hook 域表 + `core/hook/defs.rs` 行）。

**验证**：

- `cargo check --all-targets`：0 错 0 警告。
- `cargo test --all-targets`：**452 通过 / 0 失败 / 0 ignored**。
- 四入口（GUI main / pulsar-cli / pulsar-tui / lib）编译通过。
- 新增测试锁定三阶段：`registered_but_disabled_does_not_run` / `set_enabled_toggles_runtime` / `set_enabled_unknown_id_errors`；既有分发测试均改为「注册 + 显式开启」后用。
- 回归：`hook_defs_list` 出参、账本字段、裁决语义（A/B/C 纠偏）保持不变。

**未纳入本轮（按范围决策默认）**：

- 面板 / wire 未改：`hook_defs_list` 仍由定义清单（装配中的 2 条）产出，未接运行时 enabled 状态；界面切换（`hook_defs_list` 出参加 `enabled` + 切换命令 + 前端）留待独立迭代。
- 休眠 4 条维持「定义·未注册」，未做面板可见化。

## Resume / Handoff

无未完成项。后续可选项：把运行时启用状态暴露到面板/wire（需动命令出参与前端）。
