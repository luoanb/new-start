//! PTY 终端会话：基于 portable-pty 的单会话封装。
//!
//! 职责：spawn shell 伪终端、写输入、resize、kill；读线程将输出字节流转发到
//! tokio channel（由上层发射给前端），进程退出后上报退出码。
//!
//! 说明：portable-pty 为同步阻塞 API，读循环在专用线程运行，通过 channel 与
//! tokio 事件循环桥接，不阻塞 tauri command。

use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tokio::sync::mpsc;

use crate::core::{AppError, AppResult};

/// 前端监听的后端终端输出事件名（高频，独立于 state-changed）。
pub const TERMINAL_OUTPUT_EVENT: &str = "app://terminal-output";
/// 前端监听的后端终端退出事件名。
pub const TERMINAL_EXIT_EVENT: &str = "app://terminal-exit";

/// 默认终端尺寸（对齐 VS Code 集成终端的常规初始值）。
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_ROWS: u16 = 24;

/// 终端能力兜底值：父进程未给出可用 `TERM` 时，写给子 PTY。
///
/// 前端为 xterm.js（支持 256 色），取 `xterm-256color` 与真实能力一致。
const FALLBACK_TERM: &str = "xterm-256color";

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// 会话元信息（`terminal_list` 返回）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub shell: String,
    pub cwd: Option<String>,
    /// Some = 已退出（携带退出码）；None = 运行中。
    pub exit_code: Option<i32>,
}

/// 单个 PTY 终端会话。
///
/// 内部可变字段均以 `Mutex` 包裹以保持 Send + Sync，使其可放进 `Arc`
/// 供 manager / 读线程 / command 层共享。
pub struct TerminalSession {
    session_id: String,
    shell: String,
    cwd: Option<String>,
    /// 保留 master 用于 resize（writer/reader 已在 spawn 时取出）。
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    /// 由读线程在 EOF 后 take 并 wait() 取真实退出码。
    child: Arc<Mutex<Option<Box<dyn Child + Send + Sync>>>>,
    exit_code: Arc<Mutex<Option<i32>>>,
}

impl TerminalSession {
    /// 启动一个新 shell 会话。返回 (session, 输出通道, 退出通道)。
    ///
    /// 调用方负责消费两个 receiver：输出字节流逐块推给前端；
    /// 退出码到达后应把会话从 manager 移除。
    pub fn spawn(
        cwd: Option<String>,
        shell: Option<String>,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> AppResult<(Arc<TerminalSession>, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<i32>)> {
        let shell = shell.unwrap_or_else(default_shell);
        let mut builder = CommandBuilder::new(&shell);
        if let Some(dir) = &cwd {
            builder.cwd(dir);
        }
        Self::spawn_impl(cwd, shell, builder, cols, rows)
    }

    /// 启动一次性命令会话（Agent 可见执行 / 方案 A）：命令在 PTY 内执行，
    /// stdout/stderr 合并为输出流；`command` 同时用作会话展示名（tab 标题）。
    pub fn spawn_command(
        cwd: Option<String>,
        command: String,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> AppResult<(Arc<TerminalSession>, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<i32>)> {
        let mut builder = shell_command_builder(&command);
        if let Some(dir) = &cwd {
            builder.cwd(dir);
        }
        Self::spawn_impl(cwd, command, builder, cols, rows)
    }

    /// 共享的 PTY 装配：openpty → spawn builder → 取 reader/writer → 读线程。
    fn spawn_impl(
        cwd: Option<String>,
        label: String,
        mut builder: CommandBuilder,
        cols: Option<u16>,
        rows: Option<u16>,
    ) -> AppResult<(Arc<TerminalSession>, mpsc::Receiver<Vec<u8>>, mpsc::Receiver<i32>)> {
        // 终端能力兜底：宿主（prod 启动器）没给可用 TERM 时补上，否则子进程里的
        // 分页器会把 PTY 当成「非全功能终端」而报警告并停在 Press RETURN。
        apply_term_default(&mut builder);

        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: rows.unwrap_or(DEFAULT_ROWS),
                cols: cols.unwrap_or(DEFAULT_COLS),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::RuntimeError(format!("terminal: openpty failed: {e}")))?;

        let child = pair
            .slave
            .spawn_command(builder)
            .map_err(|e| AppError::RuntimeError(format!("terminal: spawn shell failed: {e}")))?;
        let master = pair.master;
        let reader = master
            .try_clone_reader()
            .map_err(|e| AppError::RuntimeError(format!("terminal: clone reader failed: {e}")))?;
        let writer = master
            .take_writer()
            .map_err(|e| AppError::RuntimeError(format!("terminal: take writer failed: {e}")))?;

        let session_id = format!("term-{:04}", SESSION_COUNTER.fetch_add(1, Ordering::Relaxed));
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(64);
        let (exit_tx, exit_rx) = mpsc::channel::<i32>(1);

        let child_shared = Arc::new(Mutex::new(Some(child)));
        let exit_code = Arc::new(Mutex::new(None));

        // 读循环：输出字节流 → output_tx；EOF 后 take child → wait() 取退出码 → exit_tx。
        let reader_child = Arc::clone(&child_shared);
        let reader_exit_code = Arc::clone(&exit_code);
        let reader_session = session_id.clone();
        std::thread::Builder::new()
            .name(format!("terminal-reader-{reader_session}"))
            .spawn(move || {
                let mut reader = reader;
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF：shell 已关闭输出
                        Ok(n) => {
                            let chunk = buf[..n].to_vec();
                            if output_tx.blocking_send(chunk).is_err() {
                                break; // 接收端已关闭
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    }
                }
                let mut guard = reader_child.lock().unwrap_or_else(|p| p.into_inner());
                let child = guard.take();
                let code = child
                    .map(|mut c| {
                        c.wait()
                            .map(|s| s.exit_code() as i32)
                            .unwrap_or(-1)
                    })
                    .unwrap_or(-1);
                *reader_exit_code.lock().unwrap_or_else(|p| p.into_inner()) = Some(code);
                let _ = exit_tx.blocking_send(code);
            })
            .map_err(|e| {
                AppError::RuntimeError(format!("terminal: spawn reader thread failed: {e}"))
            })?;

        let session = Arc::new(TerminalSession {
            session_id,
            shell: label,
            cwd,
            master: Mutex::new(Some(master)),
            writer: Mutex::new(writer),
            child: child_shared,
            exit_code,
        });
        Ok((session, output_rx, exit_rx))
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn shell(&self) -> &str {
        &self.shell
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn exit_code(&self) -> Option<i32> {
        *self.exit_code.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// 写输入字节（前端按键 / agent 注入命令）。
    pub fn write(&self, data: &[u8]) -> AppResult<()> {
        let mut guard = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        guard
            .write_all(data)
            .map_err(|e| AppError::RuntimeError(format!("terminal: write failed: {e}")))
    }

    /// 调整终端尺寸。
    pub fn resize(&self, cols: u16, rows: u16) -> AppResult<()> {
        let guard = self.master.lock().unwrap_or_else(|p| p.into_inner());
        let master = guard.as_ref().ok_or_else(|| {
            AppError::RuntimeError("terminal: master already taken".into())
        })?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| AppError::RuntimeError(format!("terminal: resize failed: {e}")))
    }

    /// 强制结束会话（幂等：已退出则忽略）。
    pub fn kill(&self) -> AppResult<()> {
        if self.exit_code().is_some() {
            return Ok(());
        }
        let mut guard = self.child.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(child) = guard.as_mut() {
            let _ = child.kill();
        }
        Ok(())
    }

    /// 会话信息快照。
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            session_id: self.session_id.clone(),
            shell: self.shell.clone(),
            cwd: self.cwd.clone(),
            exit_code: self.exit_code(),
        }
    }
}

/// `TERM` 是否可用：缺失 / 空串 / `dumb` / `unknown` 均视为「非全功能终端」——
/// 分页器（`less`）与 `git` 会因此报 `WARNING: terminal is not fully functional`
/// 并停在 `Press RETURN to continue`。
fn term_is_usable(term: Option<&str>) -> bool {
    let Some(value) = term else {
        return false;
    };
    let value = value.trim();
    !value.is_empty() && !matches!(value.to_ascii_lowercase().as_str(), "dumb" | "unknown")
}

/// 纯决策：给定从父进程继承来的 `TERM`，返回需要写进子进程的兜底值。
///
/// `None` = 继承值可用，不覆盖（宿主意愿优先）。
fn term_fallback(inherited: Option<&str>) -> Option<&'static str> {
    if term_is_usable(inherited) {
        None
    } else {
        Some(FALLBACK_TERM)
    }
}

/// 给即将 spawn 的 PTY 子进程补 `TERM` 兜底。
///
/// 子 PTY 默认**全量继承**父进程环境（`portable-pty` 的 `CommandBuilder` 基线即
/// `std::env::vars_os()`），而本仓不做任何 env 覆盖。于是 dev 态（Pulsar 由终端
/// 启动，继承 `xterm-256color`）正常；prod 态（由 GUI 启动器 / 宿主启动，`TERM`
/// 缺失或被设为 `dumb`）内置终端里跑 `git branch` 就会撞上分页器告警。
///
/// 此处只做「缺失即补」，不 `env_clear` / `env_remove`，也不注入 `GIT_PAGER` /
/// `PAGER` 之类 git 语义变量（见 `docs/pulsar/fileops/index.md` §5：不发明 git 语义）。
fn apply_term_default(builder: &mut CommandBuilder) {
    let inherited = std::env::var("TERM").ok();
    if let Some(term) = term_fallback(inherited.as_deref()) {
        builder.env("TERM", term);
    }
}

/// 默认 shell：Unix 优先 `$SHELL`（取不到退回 `sh`）；Windows `cmd.exe`。
fn default_shell() -> String {
    #[cfg(windows)]
    {
        "cmd.exe".to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
    }
}

/// 一次性命令的 PTY 启动方式，与 `tools/cmd_exec::build_command` 保持一致：
/// Unix `sh -c`，Windows `cmd /C`。
#[cfg(windows)]
fn shell_command_builder(command: &str) -> CommandBuilder {
    let mut builder = CommandBuilder::new("cmd");
    builder.arg("/C");
    builder.arg(command);
    builder
}

#[cfg(not(windows))]
fn shell_command_builder(command: &str) -> CommandBuilder {
    let mut builder = CommandBuilder::new("sh");
    builder.arg("-c");
    builder.arg(command);
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn spawn_shell_emits_output() {
        // 交互 shell 冒烟：PTY 下 sh 判定为 tty，应输出提示符等数据。
        let (_session, mut output_rx, _exit_rx) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        let chunk = tokio::time::timeout(Duration::from_secs(3), output_rx.recv())
            .await
            .expect("shell should emit output within 3s")
            .expect("output channel alive");
        assert!(!chunk.is_empty());
    }

    #[tokio::test]
    async fn spawn_command_echo_collects_output_and_exit_code() {
        let (_session, mut output_rx, mut exit_rx) =
            TerminalSession::spawn_command(None, "echo pty-hi".to_string(), None, None).unwrap();
        let mut all = Vec::new();
        while let Ok(Some(chunk)) = tokio::time::timeout(Duration::from_secs(5), output_rx.recv()).await
        {
            all.extend_from_slice(&chunk);
        }
        assert!(String::from_utf8_lossy(&all).contains("pty-hi"));
        let code = tokio::time::timeout(Duration::from_secs(5), exit_rx.recv())
            .await
            .expect("exit within 5s")
            .expect("exit channel alive");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn spawn_command_nonzero_exit_code() {
        let (_session, mut _output_rx, mut exit_rx) =
            TerminalSession::spawn_command(None, "exit 7".to_string(), None, None).unwrap();
        let code = tokio::time::timeout(Duration::from_secs(5), exit_rx.recv())
            .await
            .expect("exit within 5s")
            .expect("exit channel alive");
        assert_eq!(code, 7);
    }

    #[tokio::test]
    async fn write_feeds_session_input() {
        let (session, mut _output_rx, _exit_rx) =
            TerminalSession::spawn(None, Some("sh".to_string()), None, None).unwrap();
        // 写命令应成功（PTY 双向）；会话随后清理。
        session.write(b"echo ok\n").expect("write should succeed");
        let _ = session.kill();
    }

    #[test]
    fn term_is_usable_rejects_incapable_values() {
        assert!(!term_is_usable(None));
        assert!(!term_is_usable(Some("")));
        assert!(!term_is_usable(Some("   ")));
        assert!(!term_is_usable(Some("dumb")));
        assert!(!term_is_usable(Some("DUMB")));
        assert!(!term_is_usable(Some("unknown")));
        assert!(term_is_usable(Some("xterm-256color")));
        assert!(term_is_usable(Some(" linux ")));
    }

    #[test]
    fn term_fallback_only_fills_incapable_values() {
        assert_eq!(term_fallback(None), Some(FALLBACK_TERM));
        assert_eq!(term_fallback(Some("")), Some(FALLBACK_TERM));
        assert_eq!(term_fallback(Some("dumb")), Some(FALLBACK_TERM));
        // 宿主已给出可用值 ⇒ 不覆盖（`None`）。
        assert_eq!(term_fallback(Some("xterm-256color")), None);
        assert_eq!(term_fallback(Some("screen-256color")), None);
    }

    /// 端到端：父进程缺可用 `TERM` 时，子 PTY 内应看到兜底值。
    ///
    /// 父进程已有可用 `TERM`（如开发机交互终端）时跳过——那种情形的契约是
    /// 「不覆盖」，由 `term_fallback` 单测覆盖。
    #[tokio::test]
    async fn spawn_command_applies_term_fallback_when_parent_lacks_term() {
        if term_is_usable(std::env::var("TERM").ok().as_deref()) {
            return;
        }
        let (_session, mut output_rx, mut exit_rx) = TerminalSession::spawn_command(
            None,
            "echo TERM=$TERM".to_string(),
            None,
            None,
        )
        .unwrap();
        let mut all = Vec::new();
        while let Ok(Some(chunk)) =
            tokio::time::timeout(Duration::from_secs(5), output_rx.recv()).await
        {
            all.extend_from_slice(&chunk);
        }
        let text = String::from_utf8_lossy(&all);
        assert!(
            text.contains(&format!("TERM={FALLBACK_TERM}")),
            "expected fallback TERM in PTY output, got: {text:?}"
        );
        let code = tokio::time::timeout(Duration::from_secs(5), exit_rx.recv())
            .await
            .expect("exit within 5s")
            .expect("exit channel alive");
        assert_eq!(code, 0);
    }
}
