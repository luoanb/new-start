//! 上下文安全公共模块：工具结果统一上限（cap_tool_result）+ poller 熔断状态机。
//!
//! 背景：`conv_1787253076882845861` 事故——grep 返回 3MB 单条工具结果 → 上下文超限 400
//! → poller 无限空转。教训：**任何工具结果都必须上下文可控**，从工具设计与公共落库点
//! 双重约束；同时 poller 对同一失败不得无限重试。
//!
//! 防线分工：
//! - 各工具自带上限（grep 行截断 / read_file 字节封顶 / git_blame 行数上限 / neuron 上限）；
//! - 本模块提供统一兜底：任何工具结果超 `tool_result_max_chars` → head/tail 截断 + 提示。
//!   调用点：`RoundExecutor::execute_tools`（结果既落库又拼进本轮输出，一次截断两头受益）。
//! - 轮询熔断：错误分类 → 指数退避 → 熔断暂停（社区熔断三态简化为两级状态机）。
//!
//! 设计约束：截断提示必须引导模型「用更精确参数重试」，不能静默丢内容（社区
//! OpenClaw / lite_agent 经验）。熔断需「连续失败才触发 + 可配置 + 可手动恢复」，
//! 避免误伤正常会话。

use super::error::AppError;

/// 单条工具结果默认上限（字符）。社区参考 8-12K。
pub const DEFAULT_TOOL_RESULT_MAX_CHARS: usize = 12_000;
/// poller 连续失败退避阈值（社区：错误分类 → 指数退避，约 3 次开始退避）。
pub const DEFAULT_POLL_BACKOFF_AFTER: u32 = 3;
/// poller 连续失败熔断阈值（社区约 5 次/30s；此处 6 次触发暂停，需人工恢复）。
pub const DEFAULT_POLL_PAUSE_AFTER: u32 = 6;
/// 单次退避最多跳过的 tick 数（2^n 指数封顶，防止长时间静默）。
pub const DEFAULT_BACKOFF_MAX_SKIPS: u32 = 8;

/// 上下文安全配置聚合：gateway 从 config.json `context` 节构造后传入执行组件。
/// 缺省字段回落上面的 `DEFAULT_*` 常量（未配置时行为与硬编码时代一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextSafetyConfig {
    pub tool_result_max_chars: usize,
    pub poll_backoff_after: u32,
    pub poll_pause_after: u32,
    pub backoff_max_skips: u32,
}

impl Default for ContextSafetyConfig {
    fn default() -> Self {
        Self {
            tool_result_max_chars: DEFAULT_TOOL_RESULT_MAX_CHARS,
            poll_backoff_after: DEFAULT_POLL_BACKOFF_AFTER,
            poll_pause_after: DEFAULT_POLL_PAUSE_AFTER,
            backoff_max_skips: DEFAULT_BACKOFF_MAX_SKIPS,
        }
    }
}

impl ContextSafetyConfig {
    /// 由 config.json `context` 节构造；未建模/缺省字段回落内置默认。
    pub fn from_section(section: &super::config::ContextSection) -> Self {
        Self {
            tool_result_max_chars: section
                .tool_result_max_chars
                .unwrap_or(DEFAULT_TOOL_RESULT_MAX_CHARS),
            poll_backoff_after: section.poll_backoff_after.unwrap_or(DEFAULT_POLL_BACKOFF_AFTER),
            poll_pause_after: section.poll_pause_after.unwrap_or(DEFAULT_POLL_PAUSE_AFTER),
            backoff_max_skips: section.backoff_max_skips.unwrap_or(DEFAULT_BACKOFF_MAX_SKIPS),
        }
    }
}

/// 通用超长文本截断：保留 head（前 2/3 预算）+ 省略标记 + tail（尾部 1/3 预算）。
/// 为省略标记预留字符空间，保证结果 ≤ `max_chars`。供工具结果兜底与存量清理共用。
pub fn cap_text(content: &str, max_chars: usize) -> String {
    let total = content.chars().count();
    if total <= max_chars {
        return content.to_string();
    }
    // 为省略标记（固定文本 + 字符数位宽）预留空间，避免 head/tail/标记之和超预算。
    let budget = max_chars.saturating_sub(80);
    let head: String = content.chars().take(budget * 2 / 3).collect();
    let tail_start = total.saturating_sub(budget / 3);
    let tail: String = content.chars().skip(tail_start).collect();
    let omitted = total.saturating_sub(head.chars().count() + tail.chars().count());
    format!("{head}\n…[中间 {omitted} chars 省略]…\n{tail}")
}

/// 统一工具结果截断：超限时保留 head + 截断提示 + tail。
///
/// 返回的字符串永远 ≤ max_chars 左右（含提示尾部，故 head 预留提示空间）。
pub fn cap_tool_result(tool_name: &str, content: String, max_chars: usize) -> String {
    let total = content.chars().count();
    if total <= max_chars {
        return content;
    }
    let hint = format!(
        "\n[truncated: tool `{tool_name}` result {total} chars > limit {max_chars}; 已截断，如需完整内容请用更精确的参数重试（如 grep 缩小 path/glob、read_file 分页）]"
    );
    let hint_len = hint.chars().count();
    // 同时预留省略标记空间（80 字符 ≥ 固定文本 + usize 位宽），保证最终长度 ≤ max_chars。
    let budget = max_chars.saturating_sub(hint_len).saturating_sub(80);
    let head: String = content.chars().take(budget * 2 / 3).collect();
    let tail_start = total.saturating_sub(budget / 3);
    let tail: String = content.chars().skip(tail_start).collect();
    let omitted = total.saturating_sub(head.chars().count() + tail.chars().count());
    format!("{head}\n…[中间 {omitted} chars 省略]…\n{tail}{hint}")
}

/// 错误分类：重试决策的前提（社区：错误分类是重试前提，不是所有失败都值得重试）。
/// - `ContextLengthExceeded`：上下文超限，不可原样重试，必须先改上下文（L3 强制降级）；
/// - `Transient`：瞬时错误（429 / 5xx / 超时 / 网络），指数退避重试；
/// - `Permanent`：永久错误（认证 / 参数类 400 / 404），不重试，直接计入熔断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    ContextLengthExceeded,
    Transient,
    Permanent,
}

pub fn classify_error(error: &AppError) -> ErrorClass {
    match error {
        AppError::LlmRequestFailed(message) => {
            let lower = message.to_lowercase();
            if lower.contains("context length") || lower.contains("maximum context") {
                ErrorClass::ContextLengthExceeded
            } else if lower.contains("429")
                || lower.contains("rate limit")
                || lower.contains(" 500 ")
                || lower.contains(" 502 ")
                || lower.contains(" 503 ")
                || lower.contains(" 504 ")
                || lower.contains("timeout")
                || lower.contains("timed out")
                || lower.contains("request failed")
            {
                ErrorClass::Transient
            } else {
                ErrorClass::Permanent
            }
        }
        AppError::InvalidInput(_)
        | AppError::SkillNotFound(_)
        | AppError::ProviderNotFound(_)
        | AppError::ModelNotFound(_)
        | AppError::ModelNotSelected
        | AppError::ProviderAuthMissing(_)
        | AppError::NeuronNotFound(_) => ErrorClass::Permanent,
        // 其余（存储/运行时/压缩失败等）按瞬时处理：退避重试，避免一次失败就熔断。
        _ => ErrorClass::Transient,
    }
}

/// 会话级 poller 失败状态机：CLOSED（正常）→ BACKOFF（指数退避）→ COOLDOWN（熔断暂停）。
/// 对齐社区熔断三态，简化为本 poller 可表达的两级；成功即归零（等效 HALF_OPEN 探测）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionFailureState {
    /// 连续失败计数（成功即归零）。
    pub consecutive_failures: u32,
    /// 当前退避剩余跳过 tick 数。
    pub backoff_skips_remaining: u32,
    /// 最近一次错误分类。
    pub last_error_class: Option<ErrorClass>,
    /// 是否已熔断暂停（需手动恢复）。
    pub paused: bool,
}

impl SessionFailureState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 记录一次失败并推进状态机。返回是否应跳过下一次 poll tick。
    /// `backoff_after` / `pause_after` / `max_skips` 为配置参数。
    pub fn record_failure(
        &mut self,
        class: ErrorClass,
        backoff_after: u32,
        pause_after: u32,
        max_skips: u32,
    ) -> bool {
        self.consecutive_failures += 1;
        self.last_error_class = Some(class);
        if self.consecutive_failures >= pause_after {
            // COOLDOWN：熔断暂停，跳过全部 tick，等待用户手动恢复。
            self.backoff_skips_remaining = 0;
            self.paused = true;
            return true;
        }
        if self.consecutive_failures >= backoff_after {
            // BACKOFF：指数退避 2^(n)，n = 已超过退避阈值的次数，封顶 max_skips。
            let n = self.consecutive_failures - backoff_after + 1;
            self.backoff_skips_remaining = (1u32 << n.min(max_skips)).min(u32::MAX);
            return true;
        }
        false
    }

    /// 是否应跳过本次 poll tick。
    pub fn should_skip(&self) -> bool {
        self.paused || self.backoff_skips_remaining > 0
    }

    /// 消耗一个退避跳过 tick（仅在决定跳过时调用）。
    pub fn consume_skip(&mut self) {
        self.backoff_skips_remaining = self.backoff_skips_remaining.saturating_sub(1);
    }

    /// 成功（或用户手动恢复）：归零，回到 CLOSED。
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.backoff_skips_remaining = 0;
        self.paused = false;
        self.last_error_class = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_keeps_short_content_unchanged() {
        let s = "short".to_string();
        assert_eq!(cap_tool_result("grep", s, 12_000), "short");
    }

    #[test]
    fn cap_truncates_long_content_with_hint() {
        let big = "x".repeat(30_000);
        let capped = cap_tool_result("grep", big, 12_000);
        assert!(capped.chars().count() < 12_000);
        assert!(capped.contains("[truncated: tool `grep` result 30000 chars"));
        assert!(capped.contains("省略"));
    }

    #[test]
    fn cap_preserves_head_and_tail() {
        let mut big = String::new();
        big.push_str(&"H".repeat(10_000));
        big.push_str(&"M".repeat(10_000));
        big.push_str(&"T".repeat(10_000));
        let capped = cap_tool_result("read_file", big, 12_000);
        assert!(capped.starts_with("HHH"));
        assert!(capped.contains("TTT"));
        assert!(!capped.contains('M'));
    }

    #[test]
    fn cap_text_truncates_with_marker_and_fits_budget() {
        let big = "Z".repeat(50_000);
        let capped = cap_text(&big, 12_000);
        assert!(capped.chars().count() <= 12_000, "must respect budget");
        assert!(capped.contains("省略"));
        assert!(capped.starts_with("ZZZ"));
        // 未超限时原样返回。
        assert_eq!(cap_text("small", 12_000), "small");
    }
}
