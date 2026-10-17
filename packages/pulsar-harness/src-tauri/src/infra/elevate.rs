//! 平台提权命令构造：需要 root 的命令经用户授权后，交由系统原生提权程序重跑。
//!
//! 与业务语义无关（依赖规则见 `infra/mod.rs`）：本模块只把「原命令 + 工作目录」
//! 翻译成一条**可由现有 shell runner 直接执行**的提权命令字符串，从而复用既有的
//! 超时夹紧 / kill / 输出截断 / 日志脱敏护栏。密码由系统弹窗（polkit / macOS
//! 管理员提示）收集，不进入本进程。

use std::path::PathBuf;

/// POSIX 单引号转义：外部单引号包裹，内部单引号写成 `'\''`。
pub(crate) fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// AppleScript 字符串字面量转义（双引号包裹）：反斜杠与双引号需转义。
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn as_quote(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', r"\\").replace('"', "\\\""))
}

/// 提权脚本正文：显式切到目标工作目录后再执行原命令。
///
/// pkexec / osascript 都不保证继承调用方的工作目录，因此 cwd 必须写进脚本自身。
fn inner_script(command: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(dir) => format!("cd {} && {}", sh_quote(dir), command),
        None => command.to_string(),
    }
}

/// 在 PATH 中查找可执行文件（不引入额外依赖）。
fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// 平台原生提权是否可用：不可用时调用方应跳过授权询问，维持原失败结果。
pub fn escalation_available() -> bool {
    imp::available()
}

/// 生成提权后的 shell 命令字符串（不可用 → `None`）。
///
/// 返回值形态是「一条 shell 命令」，调用方按原有可见性（PTY / 隐藏）执行即可。
pub fn escalated_command(command: &str, cwd: Option<&str>) -> Option<String> {
    imp::build(command, cwd)
}

/// Linux：polkit 的 `pkexec`（依赖桌面 polkit agent 提供密码框）。
#[cfg(target_os = "linux")]
mod imp {
    use super::{find_in_path, inner_script, sh_quote};

    const PKEXEC: &str = "pkexec";
    /// 提权脚本的解释器：固定绝对路径，不依赖提权后被清洗的 PATH。
    const SHELL: &str = "/bin/sh";

    pub(super) fn available() -> bool {
        find_in_path(PKEXEC).is_some()
    }

    /// 纯构造函数（程序路径由参数注入，便于测试）。
    pub(super) fn build_with(program: &str, command: &str, cwd: Option<&str>) -> String {
        format!(
            "{} {} -c {}",
            sh_quote(program),
            SHELL,
            sh_quote(&inner_script(command, cwd))
        )
    }

    pub(super) fn build(command: &str, cwd: Option<&str>) -> Option<String> {
        let program = find_in_path(PKEXEC)?;
        Some(build_with(&program.display().to_string(), command, cwd))
    }
}

/// macOS：`osascript ... with administrator privileges`（系统管理员提示框）。
#[cfg(target_os = "macos")]
mod imp {
    use super::{as_quote, find_in_path, inner_script, sh_quote};

    const OSASCRIPT: &str = "osascript";

    pub(super) fn available() -> bool {
        find_in_path(OSASCRIPT).is_some()
    }

    /// 纯构造函数（程序路径由参数注入，便于测试）。
    pub(super) fn build_with(program: &str, command: &str, cwd: Option<&str>) -> String {
        let script = format!(
            "do shell script {} with administrator privileges",
            as_quote(&inner_script(command, cwd))
        );
        format!("{} -e {}", sh_quote(program), sh_quote(&script))
    }

    pub(super) fn build(command: &str, cwd: Option<&str>) -> Option<String> {
        let program = find_in_path(OSASCRIPT)?;
        Some(build_with(&program.display().to_string(), command, cwd))
    }
}

/// 其他平台（含 Windows）：v1 不做提权（UAC 无法回收 stdout/stderr），保持失败。
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
mod imp {
    pub(super) fn available() -> bool {
        false
    }

    pub(super) fn build(_command: &str, _cwd: Option<&str>) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_quote_escapes_single_quotes() {
        assert_eq!(sh_quote("plain"), "'plain'");
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
    }

    #[test]
    fn as_quote_escapes_quotes_and_backslashes() {
        assert_eq!(as_quote(r#"a"b"#), r#""a\"b""#);
        assert_eq!(as_quote(r"a\b"), r#""a\\b""#);
    }

    #[test]
    fn inner_script_prepends_cwd() {
        assert_eq!(inner_script("ls", None), "ls");
        assert_eq!(inner_script("ls", Some("/tmp")), "cd '/tmp' && ls");
    }

    /// 用真实 `sh -c printf` 回环验证转义可执行：输出必须等于原文。
    #[cfg(unix)]
    fn printf_round_trip(text: &str) -> String {
        let out = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", sh_quote(text)))
            .output()
            .expect("spawn sh");
        String::from_utf8(out.stdout).expect("utf8 stdout")
    }

    #[cfg(unix)]
    #[test]
    fn sh_quote_round_trips_through_shell() {
        for sample in [
            "plain",
            "a'b",
            "it's a 'test'",
            "/tmp/dir with space",
            "echo $HOME && ls",
            "back\\slash",
        ] {
            assert_eq!(printf_round_trip(sample), sample, "sample: {sample}");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_escalated_command_shape() {
        let built = imp::build_with("/usr/bin/pkexec", "mkdir /opt/probe", Some("/tmp/dir with space"));
        assert_eq!(
            built,
            r"'/usr/bin/pkexec' /bin/sh -c 'cd '\''/tmp/dir with space'\'' && mkdir /opt/probe'"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn escalated_command_follows_pkexec_availability() {
        // 与环境一致：pkexec 存在则给出命令，否则返回 None（调用方据此跳过询问）。
        assert_eq!(escalated_command("ls", None).is_some(), escalation_available());
    }
}
