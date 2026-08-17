# Spec: 脚本运行时第一步——mlua 集成（能执行代码）

## Goal

- 要解决什么问题：为「pulsar 作为类 Node.js 运行时」铺第一块地基——集成 mlua（Rust 对 Lua 的高层绑定），让后端具备"执行一段 Lua 代码"的最小能力。本步只解决"能执行代码"，宿主 API / 事件订阅 / 模块系统 / 脚本文件管理 / 接入 ToolRegistry / 独立存储一律后置。
- 验收结果：`cargo add mlua`（vendored + lua54）后，新增独立顶层模块 `runtime/script_engine.rs`（与 `core`/`net`/`tui` 平行）提供最小执行封装（`new()` + `eval(&str) -> AppResult<serde_json::Value>`），单测覆盖算术/字符串/函数定义/错误传播；`cargo check` 与 `cargo test --lib` 通过。

## Done Contract

- 完成定义：
  1. `src-tauri/Cargo.toml` 添加 `mlua` 依赖（`vendored` + `lua54` feature）。
  2. 新建独立顶层模块 `src-tauri/src/runtime/`（**不放入 `core/`**，与 `core`/`net`/`tui` 平行的顶级目录，体现"运行时"是独立子系统）：`runtime/mod.rs` 声明子模块 + `runtime/script_engine.rs` 提供 `ScriptEngine`（内部持 `mlua::Lua`），`new() -> AppResult<Self>` 与 `eval(source: &str) -> AppResult<serde_json::Value>`；Lua 错误统一折叠为 `AppError::RuntimeError`。
  3. `lib.rs` 顶部导出 `pub mod runtime;`。
  4. 单测：算术表达式求值、字符串拼接、Lua 函数定义并调用、非法 Lua 返回可读错误。
- 由什么证明：`cargo test --lib` 新用例全绿 + `cargo check` 0 error。
- 哪些情况仍算未完成：宿主 API 注入、事件订阅、模块 import、`scripts/` 目录与声明文件、脚本注册进 `ToolRegistry`、脚本私有存储——本步均不涉及，属后续迭代。

## 背景与根因

- 前序讨论（2026-08-09）：经 rhai / mlua / rquickjs / wasm 谱系对比，"类 Node.js 运行时"定位要求脚本能异步调用宿主 async API（`call_model`、`http_get` 等），rhai 无 async/await 语法不满足，mlua 的 `async` feature 原生支持 Rust future ↔ Lua 协程，故引擎选型落在 mlua。
- 本步不引入 `async` feature：仅"能执行代码"，保持最小；异步桥在宿主 API 步一并设计。

## 接口契约设计

```rust
// runtime/script_engine.rs
pub struct ScriptEngine {
    lua: mlua::Lua,
}

impl ScriptEngine {
    /// 创建独立 Lua VM（每个引擎实例一个 VM，避免状态互相污染）。
    pub fn new() -> AppResult<Self>;
    /// 执行一段 Lua 源码，返回其最后一个表达式的 JSON 值。
    pub fn eval(&self, source: &str) -> AppResult<serde_json::Value>;
}
```

- `mlua::Lua` 非 `Send`，实例不做共享；本步由调用方持有。并发模型在宿主 API 步定义。
- 值转换：`eval` 结果经 `serde_json` 可转换类型（number/string/boolean/nil/array/table）映射为 JSON；不可转换类型返回可读错误。

## 改动点

| 文件 | 改动 |
|---|---|
| `src-tauri/Cargo.toml` | `mlua = { version = "0.11", features = ["vendored", "lua54"] }`（版本以 `cargo add` 实际解析为准） |
| `src-tauri/src/runtime/script_engine.rs` | 新建：`ScriptEngine` + `new`/`eval` + 值转换 + 错误折叠 + 单测 |
| `src-tauri/src/runtime/mod.rs` | 新建：`pub mod script_engine;` |
| `src-tauri/src/lib.rs` | 顶部新增 `pub mod runtime;`（与 core/net/tui 平行） |

## 兼容性

- 纯增量：不触碰现有 `Tool` / `Gateway` / `cmd_exec` / `mcp` 等任何行为。
- 构建风险：`mlua` vendored 需要 C 工具链（cc），仓库已有 `rusqlite bundled` 依赖，工具链可用，风险低。

## Validation

- `cargo check`：0 error。
- `cargo test --lib`：新增 script_engine 单测全绿（算术、字符串、函数定义、错误传播）。
- 手动验证：`cargo test --lib script_engine` 观察通过。

## Change Log

- 2026-08-09：初始 micro-spec。决策：引擎选型 mlua（异步互操作能力），本步仅"能执行代码"，不引入 async feature；`ScriptEngine` 为每实例独立 VM，结果转 JSON。
- 2026-08-09（实现）：`cargo add mlua@0.12.0`（vendored + lua54）；新建 `runtime/mod.rs` + `runtime/script_engine.rs`（独立顶层模块，与 core/net/tui 平行，按用户要求不放 core）；`lib.rs` 导出 `pub mod runtime;`。`ScriptEngine::eval` 支持算术/字符串/函数定义/table→JSON（数组/对象双形态），错误折叠 `AppError::RuntimeError`。实现中发现：mlua 0.12 `Lua::new()` 直接返回 `Lua`（内部 panic，非 Result）；Lua 裸表达式不能作为语句（测试改为显式 `return`）。`cargo check` 0 error；`cargo test --lib` 264 passed（含 script_engine 10 用例）。
