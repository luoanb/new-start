# Spec: Windows 终端 cwd 的 `\\?\` 前缀与子进程控制台窗口闪现

## Goal

- 要解决什么问题：
  1. Windows 上打开终端面板 / 执行命令时，`cmd.exe` 收到 `\\?\E:\workspace\new-start` 形态的 cwd，报
     「UNC 路径不受支持。默认值设为 Windows 目录」。
  2. Windows 上启动应用后陆续「闪出多个终端窗口并消失」——GUI 子系统进程 spawn 控制台程序
     （`git` / `cmd`）时被 Windows 新建可见控制台窗口。
- 验收结果：
  1. 新开终端 tab 不再出现 UNC 报错，shell 落在工作区根目录。
  2. 启动 / 刷新时不再出现闪窗；`git`、`execute_command`、MCP stdio 子进程均不可见。

## Done Contract

- 什么算完成：两处根因各有一个明确的收敛点，且带单测。
- 由什么证明：`cargo test`（新增 `infra::platform` 单测 + 既有 terminal/cmd_exec 回归）通过；
  Windows 实机新开终端无 UNC 报错、启动无闪窗。
- 哪些情况仍算未完成：仅在 Linux/macOS 上跑通、未做 Windows 实机确认。

## Scope

- In：
  - 新增 `infra/platform.rs`：`native_path`（剥 verbatim 前缀）、`hide_console_window`（CREATE_NO_WINDOW）。
  - `terminal/mod.rs::resolve_spawn_cwd` 归一化 cwd（覆盖桌面 IPC 终端 + WS 终端 + Agent 可见 PTY）。
  - `tools/cmd_exec.rs`：`resolve_cwd` 归一化、`cwd_description` 展示归一化、`build_command` 隐藏窗口。
  - `fileops/gitops/repo.rs::run_git` 隐藏窗口。
  - `tools/mcp.rs::connect_stdio` 隐藏窗口。
- Out：
  - 不改 `WorkspaceStore` 的 canonicalize 存储契约（`workspaces.json` 仍存 verbatim 形态）与
    `resolve_in_workspace` 越界护栏——避免动到「canonical 前缀比较」的一致性。
  - 不改前端 `FileExplorer` 的 repo root 前缀匹配（两侧同为 verbatim，仍自洽）。
  - 不动 `TERM` 兜底、denylist、超时/并发等既有护栏。

## Facts / Constraints

- 已确认事实：
  - `WorkspaceStore::add` 用 `std::fs::canonicalize`，Windows 上得到 `\\?\E:\...` 并持久化。
  - `resolve_spawn_cwd` 返回 `ws.root.display().to_string()`，直通 `CommandBuilder::cwd` → `cmd.exe` 报 UNC。
  - `Release` 构建 `#![windows_subsystem = "windows"]`（`src/main.rs:2`），无控制台；
    `git`（`gitops/repo.rs:69`）、`cmd`（`cmd_exec.rs:422`）、MCP stdio（`mcp.rs:243`）
    均为 `tokio::process::Command`，未设 `CREATE_NO_WINDOW`。
  - 终端面板的 PTY 走 `portable-pty 0.9` ConPTY（`EXTENDED_STARTUPINFO_PRESENT`），本身不闪窗。
  - `refreshGit()` 一次会 spawn 多个 git（repos → status×N → branches/log/stash），与「闪多个」现象吻合。
  - `tokio::process::Command::creation_flags` 在 Windows 下可用（tokio 1.53.1 已确认）。
- 技术/业务约束：`hide_console_window` 仅隐藏窗口，不影响 stdio 管道；`native_path` 在非 Windows 为恒等。
- 已知风险：`native_path` 对非 UTF-8 路径不做转换（原样返回），避免 lossy 破坏路径。

## Open Questions

- [ ] 无（根因与收敛点已由代码确认）

## Restated Understanding

- 我理解当前任务是：修两个 Windows 特有缺陷，根因分别是「路径穿越进程边界时保留了 verbatim 前缀」与
  「GUI 子系统 spawn 控制台子进程未隐藏控制台」。
- 当前核心目标是：让终端面板 / 命令执行在 Windows 上既落在正确工作目录、又不再弹控制台窗口。
- 当前边界是：只在「子进程边界」归一化路径与隐藏窗口，不动存储层的 canonicalize 契约。
- 暂不处理：verbatim 形态在 UI 文案中的外观、`git -C` 的路径形态、跨平台其它 spawn 点。

## 接口契约设计

```rust
// infra/platform.rs —— 与业务无关的平台差异

/// 剥掉 Windows verbatim 前缀（`\\?\E:\x` → `E:\x`；`\\?\UNC\s\m` → `\\s\m`）；
/// 非 Windows / 无前缀 / 非 UTF-8 ⇒ 原样返回。
pub fn native_path(path: &Path) -> PathBuf;

/// 让隐藏执行的子进程不新建控制台窗口（Windows: CREATE_NO_WINDOW；其它平台 no-op）。
pub fn hide_console_window(cmd: &mut tokio::process::Command);
```

## Goal Alignment Check

- 当前动作是否仍服务于核心目标：是。两个改动分别直接消除两个现象的成因。
- 若否，偏差在哪里：无。
- 是否需要调整本轮目标或范围：否。

## Checkpoint Summary

- 当前任务理解：Windows 终端 UNC 报错 + 启动闪窗，两个独立根因。
- 当前核心目标：Windows 上终端可用、无闪窗。
- 当前进度：根因已定位并核对到具体行号；待批准后实施。
- 下一步 1：新增 `infra/platform.rs` 两个 helper + 单测。
- 下一步 2：接入 5 个调用点（terminal/cmd_exec/gitops/mcp）并跑 `cargo test`。
- 涉及文件 / 模块：`infra/mod.rs`、`infra/platform.rs`、`terminal/mod.rs`、`tools/cmd_exec.rs`、
  `fileops/gitops/repo.rs`、`tools/mcp.rs`。
- 风险：低。`resolve_spawn_cwd` 既有单测在 Windows 下需同步期望值（改为归一化后的路径）。
- 验证方式：`cargo test`；Windows 实机开终端、启动观察。
- Execution Approval: `Approved`（2026-09-15，用户批准全部实施）

## Change Log

- 2026-09-15: 初始记录（根因定位 + 方案，待批准）。
- 2026-09-15: 获批后实施。新增 `infra/platform.rs`（`native_path` / `hide_console_window`）；
  接入 `terminal/mod.rs::resolve_spawn_cwd`、`tools/cmd_exec.rs`（`resolve_cwd` / `cwd_description` /
  `build_command`）、`fileops/gitops/repo.rs::run_git`、`tools/mcp.rs::connect_stdio`；
  同步修订 `terminal/mod.rs` 的 `falls_back_to_active_workspace_root` 单测并新增
  `explicit_verbatim_cwd_is_normalized`。

## Validation

- Self-check: 5 个调用点覆盖「PTY spawn / 隐藏 shell / git / MCP」全部 Windows 控制台子进程入口。
- Static checks: `cargo check --no-default-features --lib --tests` ✅ 通过（含新增单测的类型检查）。
- Runtime / Test: ❌ 未取得。两个环境障碍（均与本次改动无关）：
  1. 默认 `target` 下 `debug/build/wry-8d1ecc6b8379e6a6/build-script-build.exe` 被占用，
     删除/重命名均「拒绝访问」，且工作区内无相关存活进程（疑为安全软件残留句柄）⇒ `cargo test` 无法在该 target 重建。
  2. 改用独立 `target-verify`：编译成功（5m06s），但测试二进制启动即
     `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND`，命令行直跑同样失败并报
     `process launch failed` ⇒ 当前环境无法执行新编译的测试 exe。
  - 另：`cargo check --all-targets` 因 `net/static_assets.rs` 缺 `../build/`（未执行 `pnpm build`）失败，
    亦为既有环境问题。
- Human confirmation: 待 Windows 实机复验（新开终端无 UNC 报错、启动无闪窗）。
- 结果汇总：代码改动完成且静态检查通过；运行时验证受环境阻塞，未取得测试证据。
- 核心目标是否已由证据证明完成：未完成（缺运行时/人工验收证据）。
- 若未完成，当前剩余差距：`cargo test` 结果 + Windows 实机现象复验。
- 剩余风险：低。仅字符串归一化与创建标志设置，无逻辑分支变化。

## Resume / Handoff

- 当前状态：代码已改完，静态检查通过；等待可用的测试环境与实机复验。
- 当前卡点：`target` 目录文件占用导致 `cargo test` 无法重建/运行（环境问题，非代码问题）。
- 下一步唯一动作：清理占用后跑
  `cargo test --no-default-features --lib -- platform terminal cmd_exec`。
- 下一轮核心目标：Windows 实机复验两处现象消失。
