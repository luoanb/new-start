# Spec: 修复——内置终端 TERM 兜底（生产版 git 分页警告）

来源：用户报告「开发版正常，构建的生产版在内置终端跑 `git branch` 报 `WARNING: terminal is not fully functional` / `Press RETURN to continue`」（2026-09-13）。

## Goal

- 要解决什么问题：内置终端的 PTY 子进程**全量继承**启动 Pulsar 的父进程环境（`portable-pty` 的 `CommandBuilder` 环境基线 = `std::env::vars_os()`），而本仓从未设置 `TERM`。于是：
  - dev：Pulsar 由交互终端启动 → 继承 `xterm-256color` → 正常；
  - prod：Pulsar 由 GUI 启动器 / 桌面环境 / 自动化宿主启动 → `TERM` 缺失或为 `dumb` → 子 PTY 内 `git` 按 `pager`(→`less`) 起分页器时判定「非全功能终端」，打印警告并停在 `Press RETURN to continue`。
- 验收结果：
  1. 父进程 `TERM` 缺失 / 空 / `dumb` / `unknown` 时，内置终端内 `echo $TERM` 得到可用值，`git branch` 不再出现该警告；
  2. 父进程已有**可用** `TERM`（如 `xterm-256color`）时行为完全不变——不覆盖宿主意愿；
  3. `spawn`（用户手动会话）与 `spawn_command`（Agent 一次性会话）两条路径一致生效。

## Done Contract

- 什么算完成：`spawn_impl` 这一唯一装配点在缺能力时补 `TERM`；判定逻辑为可单测的纯函数；父进程已给有效值时零副作用。
- 由什么证明：`cargo test --lib terminal::session` 全绿（含新增纯函数单测与 PTY 级端到端断言）；`cargo check --lib --tests` 无 error；App 内（prod 启动）跑 `echo $TERM` / `git branch` 人工确认。
- 哪些情况仍算未完成：仅改代码未编译验证；或覆盖了父进程已设置的有效 `TERM`；或顺手注入了 `GIT_PAGER`/`PAGER`（越界，见 Facts）。

## Scope

- In：`packages/pulsar-app/src-tauri/src/terminal/session.rs` —— 兜底判定纯函数、`spawn_impl` 装配点调用、单测。
- Out：
  - `docs/pulsar/terminal/index.md`（🔒 盖章）不改；
  - 不注入 `GIT_PAGER` / `PAGER` / `core.pager`（Git 语义，越界）；
  - 不动前端的 xterm 主题与 `terminal_spawn` 的 IPC 契约（入参仍只有 `cwd/shell/cols/rows`）；
  - 不做 terminfo 探测（不判断目标机是否缺 `xterm-256color` 条目，见「剩余风险」）。

## Facts / Constraints

- 环境继承实证：`portable-pty 0.9.0` → `src/cmdbuilder.rs:74-75` `get_base_env()` = `std::env::vars_os()`；`:218/:230/:257` `envs: get_base_env()`。本仓对 `src-tauri/src` 检索 `dumb|env_insert|env_remove|env_clear|setenv` **0 命中**，即无任何 env 覆盖。
- 触发链路：`git branch` 只要 stdout 是 TTY 就起分页器（与是否满屏无关）；`git var GIT_PAGER` 实测回退为编译期默认 `pager`（`/usr/bin/pager`→`less`）；`TERM=dumb` 时 `tput colors=-1`，less 报 `terminal is not fully functional`。
- `docs/pulsar/fileops/index.md` §5「Git 约定」（🔒）：「用户命令行里的 git」是唯一真相、本领域**不发明 git 语义** ⇒ 本次修复**不得**注入 `GIT_PAGER`/`PAGER` 等 git 语义变量，只补终端能力变量 `TERM`。
- `docs/pulsar/terminal/index.md` §2（会话单元：spawn→write→resize→kill）、§4（Agent 走同一管理器）均为 🔒。补 `TERM` 不改变会话单元、生命周期、事件流与 spawn 入参语义，**与盖章章节不冲突**，故本次不改该文档；若用户认为应把「环境兜底」写进域约定，需先解章。
- 前端为 xterm.js（`@xterm/xterm ^6`），支持 256 色 ⇒ 兜底值取 `xterm-256color`。

## Restated Understanding

- 我理解当前任务是：让内置终端在「宿主没给终端能力」时仍然可用，从而消灭生产版的 `git branch` 分页警告。
- 当前核心目标是：把 `TERM` 兜底收敛到 PTY 装配的唯一入口，且只兜底、不夺权。
- 当前边界是：只动 `session.rs` 一个文件；不碰盖章文档、不碰 git 语义、不碰 IPC 契约。

## 接口契约设计

```rust
/// 终端能力兜底值：内置终端前端是 xterm.js（256 色）。
const FALLBACK_TERM: &str = "xterm-256color";

/// `TERM` 是否可用：缺失 / 空串 / `dumb` / `unknown` 均视为「非全功能终端」。
fn term_is_usable(term: Option<&str>) -> bool;

/// 纯决策：给定继承来的 `TERM`，返回需要写入子进程的兜底值（`None` = 不覆盖）。
fn term_fallback(inherited: Option<&str>) -> Option<&'static str>;

/// 把兜底值写进 builder（读取当前进程 `TERM`，仅在不可用时设置）。
fn apply_term_default(builder: &mut CommandBuilder);

/// 唯一装配点：openpty → spawn builder → reader/writer → 读线程。
fn spawn_impl(cwd, label, mut builder: CommandBuilder, cols, rows) -> ...;
```

- `term_fallback` 是纯函数 ⇒ 判定逻辑可确定性单测，不依赖父进程环境。
- `apply_term_default` 只调用 `builder.env("TERM", ...)`，**从不** `env_clear`/`env_remove` ⇒ 其他环境变量继承行为不变。

## Checkpoint Summary

- 当前任务理解：生产版内置终端 TERM 缺能力 → git 分页器告警；给子 PTY 补 TERM。
- 当前核心目标：装配点一处兜底，只兜底不覆盖。
- 当前进度：spec 落盘 + 代码完成，静态与单测验证通过，待 prod 人工确认。
- 涉及文件 / 模块：`terminal/session.rs`（`spawn_impl`、`shell_command_builder` 调用方不变）。
- 风险：目标机 terminfo 无 `xterm-256color` 条目时兜底值仍会失效（Linux 发行版标准 ncurses 库均含，罕见）。
- 验证方式：`cargo test --lib terminal::session`；App（prod 启动）内 `echo $TERM` + `git branch`。
- Execution Approval: 用户「修复」指令（2026-09-13，承接上一轮给出的方案 1）。

## Change Log

- 2026-09-13：初始记录。`session.rs` 新增 `FALLBACK_TERM` / `term_is_usable` / `term_fallback` / `apply_term_default`，在 `spawn_impl` 统一应用。

## Validation

- Self-check: 定位到「装配点唯一（`spawn_impl`）+ 父进程无 env 覆盖」；兜底只**增** `TERM` 一项，未 `env_clear`/`env_remove`，未注入 `GIT_PAGER`/`PAGER`。
- Static checks: `cargo test --lib terminal::session` 编译通过 → 0 error。
- Runtime / Test:
  - `cargo test --lib terminal::session` → `7 passed; 0 failed`（含新增 `term_is_usable…`、`term_fallback_only_fills_incapable_values`、`spawn_command_applies_term_fallback_when_parent_lacks_term`）。
  - `cargo test --lib` → `502 passed; 0 failed`（无回归）。
  - 真实 PTY 复现/消证（`script -qc …` 分配 pty，`git --paginate branch` 强制分页）：
    - `TERM=dumb` → `WARNING: terminal is not fully functional` + `Press RETURN to continue`（复现原始缺陷）；
    - `TERM=xterm-256color` → 无警告，正常着色输出。
- 补充（prod 条件压测，2026-09-13）：
  - `TERM=dumb cargo test --lib terminal::session` → `7 passed; 0 failed`；端到端用例**未被跳过**，即在 prod 的 `dumb` 条件下子 PTY 确实拿到兜底值。
  - 入口覆盖：`terminal/commands.rs:61`（桌面 IPC）、`terminal/ws.rs:73`（浏览器 / WS）、`tools/cmd_exec.rs:346`（Agent 一次性命令）三处**全部**汇入 `TerminalSession::spawn*`——同一装配点，无第二条旁路。
- Human confirmation: 待用户在生产构建内确认（`echo $TERM` 有值；`git branch` 无警告）。
- 结果汇总：自动化证据齐（单测 7/7、全量 502/502、PTY 前后对照、prod 条件复跑）；人工确认待补。
- 核心目标是否已由证据证明完成：否（差 prod 人工确认）。
- 剩余风险：term 条目缺失的极端环境；`TERM` 与实际终端能力不匹配时的颜色/光标渲染差异（xterm.js 能力 ≥ xterm-256color，不构成风险）。

## Resume / Handoff

- 当前状态：修复完成，待验证收尾。
- 当前卡点：无（自动验证可通过），仅剩 prod 人工确认。
- 下一步唯一动作：在 prod 构建的内置终端执行 `echo $TERM` 与 `git branch`。
- 下一轮核心目标：如需，评估把「环境兜底」写入 `docs/pulsar/terminal/index.md`（须用户解章）。
