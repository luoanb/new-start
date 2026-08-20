use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

use crate::terminal::{AgentTerminalBridge, TerminalSession};

use super::{
    error::{AppError, AppResult},
    tool_registry::Tool,
};

/// v1 保守安全策略：命中以下前缀（trim 后）的命令直接拒绝。
pub(crate) const DANGEROUS_PREFIXES: &[&str] = &[
    "rm -rf /",
    "rm -rf /*",
    "mkfs",
    "dd of=/dev/",
    "dd if=/dev/zero of=/dev/",
    ":(){",
    "chmod -R 777 /",
    "> /dev/sda",
    "fdisk /dev/",
    "pvcreate /dev/",
];

/// 需要词边界的前缀（避免误伤 shutdowner / sudoku 等普通词）。
pub(crate) const DANGEROUS_WORD_PREFIXES: &[&str] = &[
    "sudo",
    "shutdown",
    "reboot",
    "poweroff",
    "halt",
    "cryptsetup",
];

pub(crate) fn is_denied(command: &str) -> bool {
    let trimmed = command.trim();
    DANGEROUS_PREFIXES.iter().any(|p| trimmed.starts_with(p))
        || DANGEROUS_WORD_PREFIXES.iter().any(|w| {
            trimmed.starts_with(w)
                && match trimmed[w.len()..].chars().next() {
                    None => true,
                    Some(c) => c.is_whitespace(),
                }
        })
}

pub(crate) const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub(crate) const MIN_TIMEOUT_MS: u64 = 1_000;
pub(crate) const MAX_TIMEOUT_MS: u64 = 120_000;
pub(crate) const MAX_CONCURRENT: usize = 4;
pub(crate) const MAX_OUTPUT_CHARS: usize = 64 * 1024;
pub(crate) const TIMEOUT_EXIT_CODE: i32 = 124;

/// 执行系统 shell 命令的 Agent 工具。
pub struct ExecuteCommandTool {
    semaphore: Semaphore,
    /// 方案 A 桥接：注入后 `visible_terminal: true` 的命令在独立 PTY 会话中
    /// 执行并实时广播到 Terminal 面板；未注入（如单元测试）时降级为隐藏执行。
    terminal: Option<Arc<AgentTerminalBridge>>,
}

impl ExecuteCommandTool {
    pub fn new() -> Self {
        Self {
            semaphore: Semaphore::new(MAX_CONCURRENT),
            terminal: None,
        }
    }

    /// 注入终端桥接（构建时调用，幂等：重复调用覆盖）。
    pub fn with_terminal(mut self, bridge: Arc<AgentTerminalBridge>) -> Self {
        self.terminal = Some(bridge);
        self
    }
}

impl Default for ExecuteCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn truncate_output(bytes: &[u8], max_chars: usize) -> String {
    let text = String::from_utf8_lossy(bytes);
    if text.chars().count() <= max_chars {
        return text.into_owned();
    }
    let truncated: String = text.chars().take(max_chars).collect();
    format!("{truncated}\n[truncated: output exceeds {max_chars} chars]")
}

pub(crate) async fn read_pipe<R: AsyncReadExt + Unpin>(reader: Option<R>) -> AppResult<Vec<u8>> {
    match reader {
        Some(mut r) => {
            let mut buf = Vec::new();
            r.read_to_end(&mut buf).await.map_err(|e| {
                AppError::RuntimeError(format!("execute_command: read pipe failed: {e}"))
            })?;
            Ok(buf)
        }
        None => Ok(Vec::new()),
    }
}

#[async_trait]
impl Tool for ExecuteCommandTool {
    fn name(&self) -> &str {
        "execute_command"
    }

    fn description(&self) -> &str {
        "Execute a shell command on the user's system (sh -c on Unix, cmd /C on Windows) and return its exit code, stdout, and stderr. Use for file inspection, running scripts, or querying system state."
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "cwd": {
                    "type": "string",
                    "description": "Optional working directory; defaults to the process's current directory"
                },
                "timeout_ms": {
                    "type": "number",
                    "description": "Optional timeout in milliseconds; defaults to 30000, clamped to [1000, 120000]"
                },
                "visible_terminal": {
                    "type": "boolean",
                    "description": "Optional; when true, the command runs in a new dedicated terminal tab (visible in the Terminal panel) with output streamed live; defaults to false (hidden execution)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> AppResult<String> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "execute_command: missing required argument 'command'".into(),
                )
            })?
            .to_string();

        if command.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "execute_command: 'command' must not be empty".into(),
            ));
        }
        if is_denied(&command) {
            return Err(AppError::InvalidInput(
                "execute_command: command denied by safety policy".into(),
            ));
        }

        let cwd = args.get("cwd").and_then(|v| v.as_str()).map(str::to_string);
        let timeout_ms = args
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let visible = args
            .get("visible_terminal")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let _permit = self.semaphore.acquire().await.map_err(|e| {
            AppError::RuntimeError(format!("execute_command: semaphore acquire failed: {e}"))
        })?;

        // 方案 A：可见执行走 PTY 会话并实时广播；桥接未注入时降级为隐藏执行。
        if visible {
            if let Some(bridge) = &self.terminal {
                return run_guarded_pty(
                    "execute_command",
                    &command,
                    cwd.as_deref(),
                    Some(timeout_ms),
                    bridge,
                )
                .await;
            }
            tracing::warn!(
                tool = "execute_command",
                "visible_terminal requested but terminal bridge not injected; falling back to hidden execution"
            );
        }

        run_guarded_shell(
            "execute_command",
            &command,
            cwd.as_deref(),
            Some(timeout_ms),
        )
        .await
    }
}

/// 共享的有护栏 shell 执行器：denylist 已在调用方校验，此处负责超时夹紧、
/// spawn/timeout/kill、输出截断与日志脱敏。并发由调用方持有的 Semaphore 控制。
/// 返回 JSON 字符串 { exit_code, stdout, stderr, timed_out }。
pub(crate) async fn run_guarded_shell(
    tool_name: &str,
    command: &str,
    cwd: Option<&str>,
    timeout_ms: Option<u64>,
) -> AppResult<String> {
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS),
    );

    let started = std::time::Instant::now();

    let mut cmd = build_command(command);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::RuntimeError(format!("{tool_name}: failed to spawn: {e}")))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(status) => {
            status.map_err(|e| AppError::RuntimeError(format!("{tool_name}: wait failed: {e}")))?
        }
        Err(_elapsed) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            tracing::warn!(
                tool = tool_name,
                command_len = command.len(),
                timeout_ms = timeout.as_millis() as u64,
                "shell command timed out and was killed"
            );
            return Ok(json!({
                "exit_code": TIMEOUT_EXIT_CODE,
                "stdout": "",
                "stderr": "",
                "timed_out": true
            })
            .to_string());
        }
    };

    let stdout_bytes = read_pipe::<tokio::process::ChildStdout>(stdout).await?;
    let stderr_bytes = read_pipe::<tokio::process::ChildStderr>(stderr).await?;

    let exit_code = status.code().unwrap_or(-1);
    let stdout = truncate_output(&stdout_bytes, MAX_OUTPUT_CHARS);
    let stderr = truncate_output(&stderr_bytes, MAX_OUTPUT_CHARS);

    tracing::info!(
        tool = tool_name,
        command_len = command.len(),
        cwd = cwd.unwrap_or("."),
        exit_code,
        duration_ms = started.elapsed().as_millis() as u64,
        stdout_bytes = stdout_bytes.len(),
        stderr_bytes = stderr_bytes.len(),
        "shell command executed"
    );

    Ok(json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "timed_out": false
    })
    .to_string())
}

/// PTY 可见执行（方案 A）：命令在独立终端会话中执行，输出逐块广播到
/// `app://terminal-output`（前端 Terminal 面板实时显示），同时聚合全部输出，
/// 以与 run_guarded_shell 相同的 JSON 结构返回给模型（模型行为不变）。
///
/// 护栏复刻 run_guarded_shell：denylist 已在调用方校验；此处负责超时夹紧、
/// spawn/timeout/kill 与输出截断，日志脱敏。并发由调用方持有的 Semaphore 控制。
pub(crate) async fn run_guarded_pty(
    tool_name: &str,
    command: &str,
    cwd: Option<&str>,
    timeout_ms: Option<u64>,
    bridge: &Arc<AgentTerminalBridge>,
) -> AppResult<String> {
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .clamp(MIN_TIMEOUT_MS, MAX_TIMEOUT_MS),
    );
    let started = std::time::Instant::now();

    let (session, mut output_rx, mut exit_rx) =
        TerminalSession::spawn_command(cwd.map(str::to_string), command.to_string(), None, None)
            .map_err(|e| AppError::RuntimeError(format!("{tool_name}: failed to spawn pty: {e}")))?;
    let session_id = session.session_id().to_string();
    bridge.manager().insert(Arc::clone(&session));

    // 转发任务：输出逐块广播到前端，同时累积完整输出供模型结果使用。
    let aggregate = Arc::new(Mutex::new(Vec::<u8>::new()));
    let forward_agg = Arc::clone(&aggregate);
    let forward_bridge = Arc::clone(bridge);
    let forward_id = session_id.clone();
    let (forward_done_tx, forward_done_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        while let Some(chunk) = output_rx.recv().await {
            forward_bridge.emit_output(&forward_id, chunk.clone());
            if let Ok(mut buf) = forward_agg.lock() {
                buf.extend_from_slice(&chunk);
            }
        }
        let _ = forward_done_tx.send(());
    });

    // 等待退出码；超时则 kill 会话并返回 124（与 run_guarded_shell 语义一致）。
    let exit_code = match tokio::time::timeout(timeout, exit_rx.recv()).await {
        Ok(Some(code)) => code,
        Ok(None) => -1,
        Err(_elapsed) => {
            let _ = session.kill();
            let _ = forward_done_rx.await; // 让剩余输出先广播完，避免前端截断丢尾
            tracing::warn!(
                tool = tool_name,
                command_len = command.len(),
                timeout_ms = timeout.as_millis() as u64,
                "pty command timed out and was killed"
            );
            return Ok(json!({
                "exit_code": TIMEOUT_EXIT_CODE,
                "stdout": "",
                "stderr": "",
                "timed_out": true
            })
            .to_string());
        }
    };
    let _ = forward_done_rx.await;

    let bytes = aggregate.lock().unwrap_or_else(|p| p.into_inner());
    let stdout = truncate_output(&bytes, MAX_OUTPUT_CHARS);

    tracing::info!(
        tool = tool_name,
        command_len = command.len(),
        cwd = cwd.unwrap_or("."),
        exit_code,
        duration_ms = started.elapsed().as_millis() as u64,
        pty_bytes = bytes.len(),
        "pty command executed"
    );

    Ok(json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": "",
        "timed_out": false
    })
    .to_string())
}

#[cfg(windows)]
fn build_command(command: &str) -> Command {
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(command);
    cmd
}

#[cfg(not(windows))]
fn build_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylist_rejects_dangerous_prefixes() {
        for bad in DANGEROUS_PREFIXES {
            assert!(is_denied(bad), "expected {bad:?} to be denied");
        }
        for bad in DANGEROUS_WORD_PREFIXES {
            assert!(is_denied(bad), "expected {bad:?} to be denied");
        }
        assert!(!is_denied("echo hello"));
        assert!(!is_denied("ls -la"));
        assert!(!is_denied("sudoku --help"));
        assert!(!is_denied("halted --status"));
    }

    #[test]
    fn truncate_output_shortens_long_text() {
        let long = vec![b'a'; 2 * MAX_OUTPUT_CHARS];
        let out = truncate_output(&long, MAX_OUTPUT_CHARS);
        assert!(out.contains("[truncated"));
        let short = b"ok".to_vec();
        assert_eq!(truncate_output(&short, MAX_OUTPUT_CHARS), "ok");
    }

    #[tokio::test]
    async fn execute_echo_returns_stdout() {
        let tool = ExecuteCommandTool::new();
        let out = tool
            .execute(json!({"command": "echo hello-cmd"}))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["exit_code"], 0);
        assert!(v["stdout"].as_str().unwrap().contains("hello-cmd"));
        assert_eq!(v["timed_out"], false);
    }

    #[tokio::test]
    async fn execute_nonzero_exit_code() {
        let tool = ExecuteCommandTool::new();
        let out = tool.execute(json!({"command": "exit 7"})).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["exit_code"], 7);
    }

    #[tokio::test]
    async fn execute_timeout_kills_process() {
        let tool = ExecuteCommandTool::new();
        let out = tool
            .execute(json!({"command": "sleep 30", "timeout_ms": 1000}))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["timed_out"], true);
        assert_eq!(v["exit_code"], TIMEOUT_EXIT_CODE);
    }

    #[tokio::test]
    async fn execute_missing_command_errors() {
        let tool = ExecuteCommandTool::new();
        assert!(tool.execute(json!({})).await.is_err());
    }

    #[tokio::test]
    async fn execute_denied_command_errors() {
        let tool = ExecuteCommandTool::new();
        assert!(tool.execute(json!({"command": "rm -rf /"})).await.is_err());
    }
}
