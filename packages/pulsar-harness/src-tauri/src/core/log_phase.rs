//! 集中式日志阶段（phase）注册表。
//!
//! 这是全项目日志 `phase = "..."` 字段的唯一事实来源。任何需要打 `phase` 的
//! tracing 调用都应引用这里的常量，避免散落的字符串字面量（拼写错误、异名）。
//!
//! 前端开发日志面板通过新增命令 `logs_phases` 拉取 [`all_phases`] 的值做下拉。
//! 新增 log phase 时：在此加一个 `pub const`，并追加到 [`ALL`]（含分组与说明），
//! 前端无需改动即可自动出现。

/// 一个 phase 的注册信息：`value`（写入日志的字符串）+ 分组 + 中文说明。
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct PhaseInfo {
    pub value: &'static str,
    pub group: &'static str,
    pub label: &'static str,
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase 常量。命名规范：`PHASE_<DOMAIN>_<STEP>`，值保持与既有日志字符串一致
// （本次仅常量化，不回写既有日志输出值，运行时行为零变化）。
//
// 本包为功能无关的应用骨架，只登记「应用启动 / 配置读写 / 日志命令」三类基础设施
// 阶段；任何业务阶段常量应在具体业务包内另建注册表。
// ─────────────────────────────────────────────────────────────────────────────

// ── 应用 ──
/// 应用启动装配（setup 回调）。
pub const PHASE_APP_SETUP: &str = "app_setup";
/// 主窗口创建。
pub const PHASE_APP_WINDOW: &str = "app_window";

// ── 配置 ──
/// 读取 `config.json`。
pub const PHASE_CONFIG_READ: &str = "config_read";
/// 写回 `config.json`。
pub const PHASE_CONFIG_UPDATE: &str = "config_update";

// ── 日志 ──
/// 日志列表命令（`logs_list`）。
pub const PHASE_LOGS_LIST: &str = "logs_list";
/// 日志清空命令（`logs_clear`）。
pub const PHASE_LOGS_CLEAR: &str = "logs_clear";
/// 日志级别设置命令（`logs_set_level`）。
pub const PHASE_LOGS_SET_LEVEL: &str = "logs_set_level";

/// 全量 phase 注册清单（分组 | value | 说明）。前端据此构建 phase 下拉。
/// 保持与上方常量一一对应；新增 phase 时必须追加在此。
pub const ALL: &[PhaseInfo] = &[
    // ── 应用 ──
    PhaseInfo { value: PHASE_APP_SETUP, group: "应用", label: "启动装配" },
    PhaseInfo { value: PHASE_APP_WINDOW, group: "应用", label: "创建窗口" },
    // ── 配置 ──
    PhaseInfo { value: PHASE_CONFIG_READ, group: "配置", label: "读取配置" },
    PhaseInfo { value: PHASE_CONFIG_UPDATE, group: "配置", label: "写回配置" },
    // ── 日志 ──
    PhaseInfo { value: PHASE_LOGS_LIST, group: "日志", label: "日志列表" },
    PhaseInfo { value: PHASE_LOGS_CLEAR, group: "日志", label: "清空日志" },
    PhaseInfo { value: PHASE_LOGS_SET_LEVEL, group: "日志", label: "设置日志级别" },
];

/// 返回全量 phase 注册信息，供后端 `logs_phases` 命令暴露给前端。
pub fn all_phases() -> &'static [PhaseInfo] {
    ALL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_phase_values_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in ALL {
            assert!(seen.insert(p.value), "duplicate phase: {}", p.value);
        }
    }

    #[test]
    fn all_phases_has_nonempty_group_labels() {
        for p in ALL {
            assert!(!p.group.is_empty());
            assert!(!p.value.is_empty());
            assert!(!p.label.is_empty());
        }
    }
}
