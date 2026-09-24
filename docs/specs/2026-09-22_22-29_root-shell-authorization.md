# shell 执行新增 root 授权支持

- 日期：2026-09-22
- 深度：standard
- 关联：[cmd-exec-tool](2026-08-04_10-30_cmd-exec-tool.md)（其中「用户确认机制列为 v2」即本任务）、[terminal-panel](2026-08-20_10-00_terminal-panel.md)、[usage.md](../pulsar/tool/usage.md)

## 1. 目标

`execute_command` 当前对需要 root 的命令只能「失败/被拒绝」。本任务新增一档能力：

> 命令**因权限不足失败**后，向用户发起一次**全屏授权弹窗**；用户授权则用**系统原生提权**重跑同一条命令，拒绝则维持原失败结果。

### Done Contract

- 完成 = 权限不足失败能触发全屏弹窗；授权后经系统原生提权（密码不进应用）重跑并返回结果；拒绝/超时/无消费方维持原失败结果。
- 证明 = `cargo test --lib` 通过 + 手动冒烟（低权限命令 → 弹窗 → 授权 → 系统密码框 → 成功；拒绝 → 返回原失败）。
- 未完成 = 仅后端弹窗事件而无前端消费、或提权后结果未回到模型、或硬破坏性命令因此绕过 denylist。

## 2. 已确认决策（用户答复）

| # | 决策 |
|---|---|
| D1 | 触发条件：**命令因权限不足失败后**提示授权重试（不做「命令含 sudo 就触发」，不做模型显式声明） |
| D2 | 提权机制：**系统原生提权，密码不进应用**（Linux `pkexec` / macOS `osascript ... with administrator privileges`） |
| D3 | 授权粒度：**每条命令都弹窗**（不做会话内记忆） |
| D4 | 无前端消费方（TUI/CLI/headless 无 UI）时：**保持拒绝**，不静默提权 |

## 3. 现状与可复用契约

- denylist 在 [cmd_exec.rs](../../packages/pulsar-app/src-tauri/src/tools/cmd_exec.rs#L18-L51)：`is_denied` 命中即 `InvalidInput`；`sudo` 属 `DANGEROUS_WORD_PREFIXES`。
- 工具执行链：`ConversationRunner::run_round_full` → `CapabilityAdapter` → `Tool::execute`（async，可 await，可被取消 token 抢占）。
- 既有「执行中暂停等用户」唯一范式：[GitConfirmService](../../packages/pulsar-app/src-tauri/src/fileops/gitops/confirm.rs#L87-L165) = `oneshot` + `StateChange` 事件 + `resolve(op_id, approved)` + 60s 超时；前端 [GitConfirmHost.svelte](../../packages/pulsar-app/src/lib/components/GitConfirmHost.svelte) 全局消费队列。
- 前端已有通道：`app://state-changed`（IPC 与远程 SSE 同名），全局挂载在 [`+page.svelte`](../../packages/pulsar-app/src/routes/+page.svelte#L867)。

本任务**复刻**该范式，不复用 `ConfirmDialog.svelte`（普通 400px 模态），另建全屏组件。

## 4. 方案

### 4.1 触发判定（`cmd_exec.rs`）

新增纯函数 `looks_like_permission_denied(exit_code, stdout, stderr) -> bool`：

- 前置：`exit_code != 0` 且 `exit_code != TIMEOUT_EXIT_CODE(124)` 且原始请求非 `timed_out`。
- 命中：`stdout | stderr` 含权限不足标记（大小写不敏感）：`permission denied`、`operation not permitted`、`must be root`、`are you root`、`requires root`、`access is denied`、`权限不够`、`拒绝访问`、`需要 root`。
- 仅匹配**失败**输出，避免误触发。

`execute()` 在拿到首次执行结果后插入该判定；命中才进入授权流程。

### 4.2 提权命令构造（新增 `infra/elevate.rs`）

`infra/` 只依赖 `core` 稳定类型与标准库（见 [infra/mod.rs](../../packages/pulsar-app/src-tauri/src/infra/mod.rs#L1-L5)），本模块无业务状态，只做平台差异收敛：

- `escalation_available() -> bool`：Linux 探测 `pkexec`（`/usr/bin/pkexec` 或 PATH）；macOS 探测 `osascript`；Windows 恒 `false`（v1 不实现 UAC）。
- `escalated_command(command: &str, cwd: Option<&str>) -> Option<String>`：返回**可直接交给现有 runner 的 shell 命令字符串**，从而完整复用 `run_guarded_shell` / `run_guarded_pty`（超时夹紧、kill、截断、日志脱敏）。
  - Linux：`pkexec /bin/sh -c <q(inner)>`，其中 `inner = cd <q(cwd)> && <command>`（cwd 显式注入，不依赖 pkexec 的工作目录继承）。
  - macOS：`osascript -e 'do shell script "<ascript_escaped(inner)>" with administrator privileges'`。
  - `q()` = POSIX 单引号转义（`'` → `'\''`）；AppleScript 侧转义 `\` 与 `"`。
- 平台不可用 → `None`。

### 4.3 授权服务（新增 `tools/root_auth.rs`）

`RootAuthService`，形状对齐 `GitConfirmService`：

- `request_and_wait(command: &str, cwd: Option<&str>, stderr_excerpt: &str) -> AppResult<RootAuthOutcome>`；`RootAuthOutcome = Approved | Rejected | Unavailable`。
- `emit` 为 `None`（无消费方）→ **立即返回 `Unavailable`，不等待**（落实 D4；区别于 git 服务的等超时）。
- request_id：`root-auth-{now_ms}-{counter}`；`resolve(request_id, approved)` 为唯一入口；超时 `ROOT_AUTH_TIMEOUT = 60s`（对齐 git）。
- 广播 `StateChange::RootAuth { request_id, command, cwd, stderr_excerpt }`。

### 4.4 工具行为（`cmd_exec.rs`）

`ExecuteCommandTool` 新增 `root_auth: Option<Arc<RootAuthService>>`（`with_root_auth` 注入），流程：

1. denylist 校验（**不变**：`rm -rf /`、`mkfs`、`sudo`、`shutdown`、`reboot` 等仍直接拒绝，授权流程不能成为绕过口）。
2. 首次执行（按 `visible_terminal` 走 `run_guarded_pty` / `run_guarded_shell`）。
3. 若 `looks_like_permission_denied` 命中 **且** `root_auth` 已注入 **且** `escalation_available()`：
   - `request_and_wait`（阻塞本轮，等待用户决定；会话 stop 通过取消 token 抢占本轮）。
   - `Approved` → 用 `escalated_command(...)` 重跑（沿用同一 `visible_terminal` 模式）；
     - 重跑超时统一取 `MAX_TIMEOUT_MS`(120s)：系统密码框耗时计入本次运行，默认 30s 过紧。
     - 返回重跑结果 JSON + `"root_authorization": "approved"`。
   - `Rejected` / `Unavailable` → 返回**原失败结果** + `"root_authorization": "denied" | "unavailable"`（让模型知道别原样重试）。
4. 其余情况：结果原样返回（形状与语义不变）。

### 4.5 前端（全屏弹窗）

- 新事件 payload：`{ kind: "root_auth", request_id, command, cwd, stderr_excerpt }`。
- `dataStore`：`rootAuthQueue` 队列 + `handleStateChanged` 新增 `root_auth` 分支 + `rootAuthResolve(requestId, approved)`（resolve 后出队），风格对齐 `gitConfirmQueue` / `gitConfirm`。
- 新增 `RootAuthHost.svelte`（全屏）：`position: fixed; inset: 0; z-index` 高于普通弹窗（ConfirmDialog 为 100），铺满并压暗全屏；内容 = 警示标题 + **命令原文**（code 块）+ 工作目录 + 失败原因摘要 + 「授权后系统将弹出密码框，密码不会进入本应用」说明 + 「拒绝 / 授权」按钮。
  - **点击遮罩不关闭**；`Esc` = 拒绝；`Enter` **不绑定**（避免误触授权，安全语义优先于 ConfirmDialog 的 Enter 习惯）。
  - 视觉令牌与交互细则以 `docs/design/BitsUI.md` 为准，实现前对照。
- 挂载：`+page.svelte` 中与 `<GitConfirmHost />` 并列。
- i18n：`translations.ts` 中英各补一组键。

### 4.6 接线

- `Gateway`：新增 `root_auth: Arc<RootAuthService>`（构造时传入 `state_emit`，与 `git_service` 同源），暴露 `root_auth()`；`assemble_local_tools(...)` 增参并 `with_root_auth(...)`（3 处调用点：启动同步装配、MCP 后台装配 base、`reassemble_and_replace`）。
- 桌面命令：`lib.rs` 新增 `root_auth_resolve(request_id, approved)` 并登记进 `invoke_handler`。
- 远程 RPC：`net/rpc.rs` 新增 `"root_auth_resolve"` 臂 + `RootAuthResolveParams`（camelCase），远程 UI 复用同一事件与命令（有 UI 的远程客户端可正常授权；完全无 UI 时按 D4 走 `Unavailable`）。

## 5. 涉及文件

后端：

| 文件 | 改动 |
|---|---|
| `src-tauri/src/infra/elevate.rs` | 新增：平台提权命令构造 + 可用性探测 |
| `src-tauri/src/infra/mod.rs` | 注册 `pub mod elevate;` |
| `src-tauri/src/tools/root_auth.rs` | 新增：`RootAuthService` |
| `src-tauri/src/tools/mod.rs` | 注册 `pub mod root_auth;` |
| `src-tauri/src/tools/cmd_exec.rs` | 权限不足判定 + 授权流程 + 提权重跑 + 注入点 + 测试 |
| `src-tauri/src/core/events.rs` | 新增 `StateChange::RootAuth` |
| `src-tauri/src/application/gateway.rs` | 构造/注入服务、`root_auth()`、装配签名 |
| `src-tauri/src/lib.rs` | `root_auth_resolve` 命令 + 登记 |
| `src-tauri/src/net/rpc.rs` | RPC 臂 + 参数结构 |
| `src-tauri/inserts/execute_command.md` | 补「权限不足可经用户授权以 root 重试」与 `root_authorization` 字段说明；保留「不要用 sudo」 |

前端：

| 文件 | 改动 |
|---|---|
| `src/lib/api/types.ts` | `StateChangeKind` + `StateChangePayload` 增 `root_auth` |
| `src/lib/types.ts` | 新增 `RootAuthRequest` |
| `src/lib/api/contracts.ts` | 新增 `rootAuthResolve` |
| `src/lib/stores/dataStore.svelte.ts` | 队列 / 事件分支 / resolve |
| `src/lib/components/RootAuthHost.svelte` | 新增：全屏授权弹窗 |
| `src/routes/+page.svelte` | 挂载 host |
| `src/lib/i18n/translations.ts` | 中英文案 |

文档（Reverse Sync）：

| 文件 | 改动 |
|---|---|
| `docs/pulsar/tool/usage.md` §3 | 「提权类命令按词边界拒绝」补一句：非灾难性命令因权限不足失败时可经用户授权以 root 重试；`sudo` 仍拒绝 |

## 6. 风险与边界

- **denylist 不受影响**：授权流程只在「已成功 spawn 且因权限失败」后触发，硬拒绝清单仍在最前；`sudo` 前缀仍拒（模型指引：不要自己加 sudo）。
- **pkexec 依赖桌面会话**：polkit agent 缺失（纯 SSH/headless）时提权会失败 → 表现为重跑后的普通失败输出，不额外弹窗。
- **提权后失去精确 cwd**：已用 `cd <cwd> && <command>` 在提权脚本内显式切换来规避。
- **等待用户期间占用并发许可**（Semaphore(4)）：4 个并发 root 请求会占满许可；v1 接受，记录为已知项。
- **本轮被 stop 抢占**：授权等待随本轮 future 一起 drop，pending 条目由超时/prune 清理（与 git 确认同语义）；随后前端 resolve 会得到 unknown/expired 或静默失败。
- **环境变量差异**：pkexec 会清洗环境，提权后的 PATH/env 与普通执行不完全一致。

## 7. 验证方式

- 单测（`cargo test --lib`）：
  - `looks_like_permission_denied` 正/反例（含中文标记、exit 124 不触发）。
  - `escalated_command` 构造与转义（Linux/macOS 分支、cwd 注入、含单引号命令）。
  - `RootAuthService`：approved / rejected / timeout / unknown id / `emit=None` 立即 Unavailable。
  - denylist 既有单测保持通过。
- 手动冒烟（Linux 桌面）：
  1. 让 agent 执行 `mkdir /opt/pulsar-root-probe` → 期望：权限不足 → 全屏弹窗 → 点「授权」→ 系统 pkexec 密码框 → 成功后目录创建。
  2. 同命令点「拒绝」→ 无提权，模型收到原失败结果 + `root_authorization: "denied"`。
  3. `execute_command: sudo ls` → 仍被 denylist 拒绝（无弹窗）。

## 8. Change Log / Validation / Handoff

### Change Log（2026-09-22 实施）

按方案落地，未发生范围外改动：

- 新增 `src-tauri/src/infra/elevate.rs`：`escalation_available()` + `escalated_command()`，
  Linux=`pkexec /bin/sh -c '<cd <cwd> && <cmd>>'`、macOS=`osascript -e 'do shell script ... with administrator privileges'`、
  其他平台返回 `None`；`sh_quote` / `as_quote` / `inner_script` 为纯函数并有单测（含 `sh -c printf` 真实回环验证转义）。
- 新增 `src-tauri/src/tools/root_auth.rs`：`RootAuthService`（`request_and_wait` / `resolve` / 60s 超时 / `prune_expired`），
  `emit=None` 时**不等待、立即 `Unavailable`**；超时/通道断裂按 `Rejected` 收敛。
- `core/events.rs`：新增 `StateChange::RootAuth { request_id, command, cwd, stderr_excerpt }` + 序列化单测。
- `tools/cmd_exec.rs`：新增 `looks_like_permission_denied` / `permission_excerpt` / `with_root_authorization`；
  `execute` 拆为 `run_once` + `escalate_if_denied`；新增 `with_root_auth` 注入点；`description()` 补一句模型指引；
  denylist 与 `sudo` 拒绝语义不变。
- 接线：`gateway.rs`（字段 + `root_auth()` + `assemble_local_tools` 增参 + 3 处装配点）、
  `lib.rs`（`root_auth_resolve` 命令 + 登记）、`net/rpc.rs`（同名 RPC 臂 + `RootAuthResolveParams`）。
- 前端：`api/types.ts`（kind + payload）、`types.ts`（`RootAuthRequest`）、`api/contracts.ts`（`rootAuthResolve`）、
  `dataStore.svelte.ts`（`rootAuthQueue` + 事件分支 + resolve 后强制出队）、新增 `components/RootAuthHost.svelte`（全屏遮罩 + 命令原文 + 拒绝/授权），
  `routes/+page.svelte` 挂载，`i18n/translations.ts` 中英 `rootAuth` 命名空间。
- 文档反写：`inserts/execute_command.md`（`root_authorization` 字段 + 模型指引 + 提权说明）、
  `docs/pulsar/tool/usage.md` §3、`docs/pulsar/architecture.md`（事件 kind 列表）。

### Validation

| 项 | 结果 |
|---|---|
| `cargo test --lib` | **540 passed; 0 failed**（含新增 `infra::elevate` 6 例、`tools::cmd_exec` 12 例、`tools::root_auth` 6 例、`core::events` 序列化 1 例） |
| `cargo check --lib` | 通过，无 warning |
| `pnpm check`（svelte-check） | **0 errors**，20 warnings（均为既有文件，`RootAuthHost.svelte` 无告警） |
| `pnpm build`（vite） | 通过 |
| 手动冒烟（`mkdir /opt/...` 授权/拒绝、`sudo ls` 仍被拒） | **未执行**：需桌面会话 + 真实 pkexec 密码框，留待人工验证 |

### 偏差与遗留

- 命令计数未改：`docs/pulsar/architecture.md` 的「108 个 commands（lib.rs 103 …）」在本改动前就已过时
  （改动前 lib.rs 实际为 110 个 `#[tauri::command]`，改动后 111），该行属既有文档债，本次只同步了事件 kind 列表。
- Windows 未实现提权（`escalation_available()` 恒 false）→ 维持原失败，与 spec §4.2 一致。
- 「approved 分支」未做自动化测试：提权实际执行依赖 pkexec 与真实密码框，集成测试不可行；
  提权命令字符串构造与转义已由 `infra::elevate` 单测覆盖。

### Resume / Handoff

- 待人工冒烟（spec §7）：Linux 桌面下用 agent 执行 `mkdir /opt/pulsar-root-probe`，确认弹窗 → 授权 → pkexec 密码框 → 目录创建；
  再确认拒绝路径返回 `root_authorization: "denied"` 且目录未创建。若冒烟暴露问题，先更新本节与 §4，再改代码。
