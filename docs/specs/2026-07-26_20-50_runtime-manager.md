# Spec: 运行时会话管理 (RuntimeManager)

## Goal

新增一个纯内存的运行时会话管理器，追踪当前正在执行的会话（Agent 模式工具循环）及其执行进度。提供 Agent 工具用于查询和关闭运行中会话，TUI 中提供可视化和关闭能力。

## Done Contract

1. `RuntimeManager` 内存级管理器，`HashMap` 存储运行中会话
2. `RunningSession` 数据模型（session_id, started_at, current_step）
3. 关闭机制：`register()` 接收调用方提供的关闭回调（`Box<dyn FnOnce() + Send`）
4. 2 个 Agent 工具：`get_running_sessions`（查询列表）、`close_session`（关闭指定会话）
5. TUI 命令：`/sessions` 列表显示 `[Running]` 标记、`/close <session_id>` 关闭会话
6. 注册方（Gateway/Engine）在 `send_model_message()` 开始/结束时调用 `register()` / `unregister()`

## Scope

**In**:
- RuntimeManager 结构体 + 方法（register / unregister / update_step / list / close / get）
- RunningSession 模型
- 2 个 Agent 工具 + 注册到 ToolRegistry
- Gateway 集成：创建 RuntimeManager + send_model_message 生命周期中注册/注销
- TUI：`/sessions` 列表运行中标记 + `/close <session_id>` 命令

**Out**:
- 持久化存储（运行时概念，不落盘）
- 多线程/并发安全（已通过 Arc<Mutex<>> 保证）
- 会话级别的超时自动关闭

## 数据模型

```rust
pub struct RunningSession {
    pub session_id: String,
    pub started_at: u128,
    pub current_step: Option<String>,
}
```

## API 设计

```
RuntimeManager {
    register(session_id: &str, abort: Box<dyn FnOnce() + Send>)    → AppResult
    unregister(session_id: &str)                                     → ()
    update_step(session_id: &str, step: &str)                        → AppResult
    close(session_id: &str)                                          → AppResult  // 触发回调 + 移除
    list()                                                           → Vec<RunningSession>
    get(session_id: &str)                                            → Option<RunningSession>
}
```

## 关闭流程

1. 调用方注册时传入 oneshot channel 的发送端作为 `abort` 闭包
2. 执行循环（Engine）中定期检查接收端 `try_recv()`
3. TUI `/close <id>` 或 Agent 工具 `close_session` 调用 `runtime_manager.close(id)`
4. `close()` 触发闭包（发送信号）、从 HashMap 中移除

## 工具定义

### get_running_sessions
- 参数：无
- 返回：运行中会话列表（session_id / started_at / current_step）

### close_session
- 参数：session_id
- 返回：关闭结果消息
- 如果 session 不存在则报错

## TUI 命令

| 命令 | 说明 |
|------|------|
| `/sessions` | 列表显示，运行中会话追加 `[Running]` |
| `/close <session_id>` | 关闭指定运行中会话 |

## 集成点

| 层 | 改动 |
|----|------|
| core/mod.rs | 添加 `pub mod runtime_manager;` + 导出 `RuntimeManager` / `RunningSession` |
| gateway.rs | 新增 `RuntimeManager` 字段、`new()` 中初始化、`runtime_manager()` 访问器 |
| engine.rs | 注册/注销调用（send_model_message 生命周期） |
| tool_registry.rs | 注册 2 个运行时 Agent 工具 |
| tui/commands.rs | 新增 `Close` 命令 |
| tui/app.rs | 新增 close 处理器、sessions 列表运行中标记 |

## 测试

- RuntimeManager 单元测试：register / unregister / list / close / update_step
- Gateway 集成测试：send_model_message 生命周期中注册和自动注销
