//! 平台差异收敛：Windows 特有的子进程窗口与路径形态差异。
//!
//! 与业务语义无关，`core/` 与各扩展目录均可引用（依赖规则见 `infra/mod.rs`）。

use std::path::{Path, PathBuf};

/// Windows `CreateProcess` 创建标志：不为子进程新建控制台窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 让隐藏执行的子进程不弹出控制台窗口。
///
/// GUI 子系统进程（`src/main.rs` 的 release 构建）自身没有控制台，此时 Windows 会为
/// `git` / `cmd` 这类控制台程序新建一个可见控制台窗口，表现为「终端窗口闪一下」。
/// 该标志只影响窗口可见性，不改变 stdio 管道语义。
#[cfg(windows)]
pub fn hide_console_window(cmd: &mut tokio::process::Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

/// 非 Windows 平台无控制台窗口概念，空操作。
#[cfg(not(windows))]
pub fn hide_console_window(_cmd: &mut tokio::process::Command) {}

/// 剥掉 Windows verbatim 路径前缀，得到可交给外部命令 / 展示的常规形态。
///
/// `std::fs::canonicalize` 在 Windows 上返回 `\\?\E:\...`（verbatim：禁用路径规范化）。
/// 该形态交给 `cmd.exe` 会被判为 UNC 路径并报「UNC 路径不受支持。默认值设为 Windows
/// 目录」，因此跨进程边界前必须剥掉：
///
/// - `\\?\E:\x` → `E:\x`
/// - `\\?\UNC\server\share\x` → `\\server\share\x`
/// - 非 Windows / 无前缀 / 非 UTF-8 路径 ⇒ 原样返回（避免 lossy 转换破坏路径）。
pub fn native_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str() {
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return match rest.strip_prefix(r"UNC\") {
                Some(unc) => PathBuf::from(format!(r"\\{unc}")),
                None => PathBuf::from(rest),
            };
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_path_keeps_plain_paths() {
        let plain = if cfg!(windows) {
            r"C:\work\proj"
        } else {
            "/work/proj"
        };
        assert_eq!(native_path(Path::new(plain)), PathBuf::from(plain));
    }

    #[cfg(windows)]
    #[test]
    fn native_path_strips_verbatim_prefix() {
        assert_eq!(
            native_path(Path::new(r"\\?\E:\workspace\new-start")),
            PathBuf::from(r"E:\workspace\new-start")
        );
        assert_eq!(
            native_path(Path::new(r"\\?\UNC\server\share\proj")),
            PathBuf::from(r"\\server\share\proj")
        );
    }
}
