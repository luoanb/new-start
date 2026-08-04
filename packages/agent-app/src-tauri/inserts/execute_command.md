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

## 对模型的期待

- 仅当你确实需要读取系统状态、运行脚本或检查文件时才调用，优先选择更轻量的方式。
- `command` 为必填；`cwd` 与 `timeout_ms` 可选。
- 依据 `exit_code` 与 `stdout` / `stderr` 判断命令是否成功，并向用户如实汇报结果。
- 长时间运行的命令请设置合理的 `timeout_ms`（硬顶 120000 毫秒），避免挂起整个会话。

## 忌用

- 不要执行破坏性、危险命令（格式化磁盘、关停机器、清空根目录等）——系统会直接拒绝并返回错误。
- 不要试图用 `sudo` 提权执行系统级变更。
- 不要在命令里把敏感信息（密钥、token）明文拼接后执行。
- 输出超过 64 KiB 会被截断，不要依赖超长输出做判断。

## 注意

- 命令有全局并发上限，许可耗尽时会等待。
- 超时后子进程会被强制杀掉，返回 `timed_out: true` 与 `exit_code: 124`。
- 系统日志不记录命令原文与输出正文，只记录长度与退出码等元信息。
