# execute_command

## 工具

在用户系统上执行一条 shell 命令（Unix 走 `sh -c`，Windows 走 `cmd /C`），返回退出码、stdout 与 stderr。

调用时传入 JSON 参数：

```json
{"command": "<shell 命令>", "cwd": "<可选工作目录>", "timeout_ms": 30000}
```

执行结果以 JSON 返回：

```json
{"exit_code": 0, "stdout": "...", "stderr": "...", "timed_out": false}
```

若命令因权限不足失败，平台会请用户授权以 root 重跑（见下），结果额外带 `root_authorization` 字段：

```json
{"exit_code": 1, "stdout": "", "stderr": "...Permission denied", "timed_out": false, "root_authorization": "denied"}
```

`root_authorization` 取值：`approved`（已授权并重跑，结果来自提权执行）、`denied`（用户拒绝或未在窗口期内决定）、`unavailable`（当前没有可用的授权入口）。

## 权限与 root

**本终端具备以 root 执行命令的能力。** 需要系统权限的操作（安装软件包、写入 `/etc`、管理服务、修改系统配置等）**不要**因为「可能没权限」就跳过、放弃或推给用户手动执行——直接执行即可。

- 直接执行原始命令，**不要**自己加 `sudo`（`sudo` 前缀会被直接拒绝）。
- 命令确实因权限不足失败时，平台会自动弹窗请用户授权并以 root 重跑；用户同意后你直接拿到提权执行的结果，无需再让用户手动操作一遍。
- 只有拿到 `root_authorization` 为 `denied` / `unavailable` 时，才说明本轮无法提权；此时再考虑替代方案或如实告知用户。

## 对模型的期待

- 仅当你确实需要读取系统状态、运行脚本或检查文件时才调用，优先选择更轻量的方式。
- 不要预先假设自己没有权限：任务需要 root 时照常执行，由用户决定是否授权（见上「权限与 root」）。
- `command` 为必填；`cwd` 与 `timeout_ms` 可选。
- 依据 `exit_code` 与 `stdout` / `stderr` 判断命令是否成功，并向用户如实汇报结果。
- 长时间运行的命令请设置合理的 `timeout_ms`（硬顶 120000 毫秒），避免挂起整个会话。
- 命令若因权限不足失败，最多再试一次：平台已负责向用户申请 root 授权，`root_authorization` 为 `denied` / `unavailable` 时说明用户未授权，不要原样重试。

## 忌用

- 不要执行破坏性、危险命令（格式化磁盘、关停机器、清空根目录等）——系统会直接拒绝并返回错误。
- 不要试图用 `sudo` 提权执行系统级变更（`sudo` 前缀会被直接拒绝）；需要 root 时正常调用命令即可，权限不足会由用户决定是否提权。
- 不要在命令里把敏感信息（密钥、token）明文拼接后执行。
- 输出超过 64 KiB 会被截断，不要依赖超长输出做判断。

## 注意

- 命令有全局并发上限，许可耗尽时会等待。
- 超时后子进程会被强制杀掉，返回 `timed_out: true` 与 `exit_code: 124`。
- 提权重跑由系统原生提权程序（Linux `pkexec` / macOS `osascript`）承担，密码由系统弹窗收集，不进入本应用；提权执行的超时统一放宽到 120000 毫秒。
- 系统日志不记录命令原文与输出正文，只记录长度与退出码等元信息。
