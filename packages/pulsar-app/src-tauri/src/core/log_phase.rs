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
// ─────────────────────────────────────────────────────────────────────────────

// ── Hook 执行 ──
pub const PHASE_HOOK_USER_ROUND_JUDGEMENT: &str = "user_round_judgement_hook";
pub const PHASE_HOOK_ROUND_REVIEW: &str = "round_review_hook";
pub const PHASE_HOOK_REVISE_TOPIC: &str = "revise_topic_hook";
pub const PHASE_HOOK_SCORE_FEEDBACK: &str = "score_feedback_hook";
pub const PHASE_HOOK_COMPLETE_SCOPE: &str = "complete_scope_hook";
pub const PHASE_HOOK_MATCH_TOPIC: &str = "match_topic_hook";
pub const PHASE_HOOK_COMPACTION: &str = "compaction_hook";
pub const PHASE_HOOK_SELECT_NEURON: &str = "select_neuron_hook";
pub const PHASE_HOOK_ASSISTANT: &str = "assistant_hook";

// ── 轮次执行 ──
pub const PHASE_RUN_ROUND: &str = "run_round";
pub const PHASE_RUN_ROUND_STREAM: &str = "run_round_stream";
pub const PHASE_ROUND_EXECUTE: &str = "round_execute";
pub const PHASE_RESOLVE: &str = "resolve";
pub const PHASE_RESOLVE_ROLE: &str = "resolve_role";
pub const PHASE_TOOL_AUTHORIZATION: &str = "tool_authorization";
pub const PHASE_PREEMPT_WAIT: &str = "preempt_wait";
pub const PHASE_APPLY_TOOLS: &str = "apply_tools";

// ── 神经元生命周期 ──
pub const PHASE_NEURON_BOOTSTRAP_NEURONS: &str = "bootstrap_neurons";
pub const PHASE_NEURON_BOOTSTRAP: &str = "bootstrap";
pub const PHASE_NEURON_REBOOTSTRAP: &str = "rebootstrap";
pub const PHASE_NEURON_ENSURE_SYSTEM: &str = "ensure_system_neuron";
pub const PHASE_NEURON_ENSURE_SESSION: &str = "ensure_session_neuron";
pub const PHASE_NEURON_ENSURE_CREATOR: &str = "ensure_creator";
pub const PHASE_NEURON_EVOLVE_CREATOR: &str = "maybe_evolve_creator_variants";
pub const PHASE_NEURON_ROLLBACK_VARIANT: &str = "rollback_variant_if_regressed";
pub const PHASE_NEURON_REWRITE_VARIANT: &str = "rewrite_variant";
pub const PHASE_NEURON_RECYCLE: &str = "recycle_if_over_capacity";
pub const PHASE_NEURON_RECYCLE_RUNTIME: &str = "neuron_recycle_runtime";

// ── 神经元选择/补给 ──
pub const PHASE_SELECT_CANDIDATES: &str = "select_candidates";
pub const PHASE_SELECT_CANDIDATES_DETAIL: &str = "select_candidates.detail";
pub const PHASE_SELECT_ASSISTANT_CANDIDATES: &str = "select_assistant_candidates";
pub const PHASE_SELECT_ONE: &str = "select_one";
pub const PHASE_SELECT_ONE_LINK_BACK: &str = "select_one.link_back";
pub const PHASE_SELECT_NEURON_MODEL_INPUT: &str = "select_neuron.model_input";
pub const PHASE_SELECT_NEURON_MODEL_INPUT_FULL: &str = "select_neuron.model_input.full";
pub const PHASE_SELECT_NEURON_MODEL_OUTPUT: &str = "select_neuron.model_output";
pub const PHASE_SELECT_NEURON_MODEL_DECISION: &str = "select_neuron.model_decision";
pub const PHASE_SELECT_ROLE: &str = "select_role";
pub const PHASE_FILL_CANDIDATES_BATCH: &str = "fill_candidates_batch";
pub const PHASE_GENERATE_DRAFTS: &str = "generate_drafts";

// ── 助手会话 / 轮询 ──
pub const PHASE_ASSISTANT_POLL_HANDLER: &str = "assistant_poll_handler";
pub const PHASE_ASSISTANT_STEP: &str = "assistant_step";
pub const PHASE_ASSISTANT_POLLER: &str = "assistant_poller";
pub const PHASE_ASSISTANT_CONVERSE: &str = "assistant_converse";
pub const PHASE_ASSISTANT_CONVERSE_STREAM: &str = "assistant_converse_stream";
pub const PHASE_ASSISTANT_ERROR_RESIDENCY: &str = "error_residency";
pub const PHASE_POLLER_TICK: &str = "poller_tick";
pub const PHASE_POLLER_RUNTIME: &str = "poller_runtime";
pub const PHASE_POLLER_CONFIG: &str = "poller_config";

// ── 消息 / 模型调用 ──
pub const PHASE_SEND_MODEL_MESSAGE: &str = "send_model_message";
pub const PHASE_SEND_MODEL_MESSAGE_STREAM: &str = "send_model_message_stream";
pub const PHASE_CALL_MODEL: &str = "call_model";
pub const PHASE_CALL_JUDGEMENT: &str = "call_judgement";
pub const PHASE_RUN_JUDGEMENT_ROUND: &str = "run_judgement_round";
pub const PHASE_PARSE_TOOL_CALL: &str = "parse_tool_call";
pub const PHASE_LLM_REQUEST_OUT: &str = "llm_request_out";
pub const PHASE_LLM_RESPONSE_IN: &str = "llm_response_in";
pub const PHASE_LLM_CALL_PERF: &str = "llm_call_perf";

// ── 评分反馈 ──
pub const PHASE_APPLY_SCORE_FEEDBACK: &str = "apply_score_feedback";
pub const PHASE_MANUAL_SCORE_FEEDBACK: &str = "manual_score_feedback";
pub const PHASE_SCORE_FEEDBACK_COMMAND: &str = "score_feedback_command";

// ── 会话 / 协调 ──
pub const PHASE_SESSION_COORDINATOR: &str = "session_coordinator";
pub const PHASE_STOP_SESSION: &str = "stop_session";
pub const PHASE_START_SESSION: &str = "start_session";
pub const PHASE_TOOL_CONFIG: &str = "tool_config";

/// 全量 phase 注册清单（分组 | value | 说明）。前端据此构建 phase 下拉。
/// 保持与上方常量一一对应；新增 phase 时必须追加在此。
pub const ALL: &[PhaseInfo] = &[
    // ── Hook 执行 ──
    PhaseInfo { value: PHASE_HOOK_USER_ROUND_JUDGEMENT, group: "Hook 执行", label: "用户轮合并裁决" },
    PhaseInfo { value: PHASE_HOOK_ROUND_REVIEW, group: "Hook 执行", label: "收尾轮合并复盘" },
    PhaseInfo { value: PHASE_HOOK_REVISE_TOPIC, group: "Hook 执行", label: "范围修订" },
    PhaseInfo { value: PHASE_HOOK_SCORE_FEEDBACK, group: "Hook 执行", label: "评分反馈" },
    PhaseInfo { value: PHASE_HOOK_COMPLETE_SCOPE, group: "Hook 执行", label: "收尾验收" },
    PhaseInfo { value: PHASE_HOOK_MATCH_TOPIC, group: "Hook 执行", label: "话题匹配" },
    PhaseInfo { value: PHASE_HOOK_COMPACTION, group: "Hook 执行", label: "上下文压缩" },
    PhaseInfo { value: PHASE_HOOK_SELECT_NEURON, group: "Hook 执行", label: "神经元选型" },
    PhaseInfo { value: PHASE_HOOK_ASSISTANT, group: "Hook 执行", label: "助手钩子" },
    // ── 轮次执行 ──
    PhaseInfo { value: PHASE_RUN_ROUND, group: "轮次执行", label: "执行轮次" },
    PhaseInfo { value: PHASE_RUN_ROUND_STREAM, group: "轮次执行", label: "执行轮次(流式)" },
    PhaseInfo { value: PHASE_ROUND_EXECUTE, group: "轮次执行", label: "轮次执行器" },
    PhaseInfo { value: PHASE_RESOLVE, group: "轮次执行", label: "角色解析" },
    PhaseInfo { value: PHASE_RESOLVE_ROLE, group: "轮次执行", label: "角色解析(细)" },
    PhaseInfo { value: PHASE_TOOL_AUTHORIZATION, group: "轮次执行", label: "工具授权" },
    PhaseInfo { value: PHASE_PREEMPT_WAIT, group: "轮次执行", label: "抢占等待" },
    PhaseInfo { value: PHASE_APPLY_TOOLS, group: "轮次执行", label: "应用工具" },
    // ── 神经元生命周期 ──
    PhaseInfo { value: PHASE_NEURON_BOOTSTRAP_NEURONS, group: "神经元生命周期", label: "神经源自举" },
    PhaseInfo { value: PHASE_NEURON_BOOTSTRAP, group: "神经元生命周期", label: "初始化" },
    PhaseInfo { value: PHASE_NEURON_REBOOTSTRAP, group: "神经元生命周期", label: "重新初始化" },
    PhaseInfo { value: PHASE_NEURON_ENSURE_SYSTEM, group: "神经元生命周期", label: "确保系统神经元" },
    PhaseInfo { value: PHASE_NEURON_ENSURE_SESSION, group: "神经元生命周期", label: "确保会话神经元" },
    PhaseInfo { value: PHASE_NEURON_ENSURE_CREATOR, group: "神经元生命周期", label: "确保创作者" },
    PhaseInfo { value: PHASE_NEURON_EVOLVE_CREATOR, group: "神经元生命周期", label: "演化创作者变体" },
    PhaseInfo { value: PHASE_NEURON_ROLLBACK_VARIANT, group: "神经元生命周期", label: "回滚变体" },
    PhaseInfo { value: PHASE_NEURON_REWRITE_VARIANT, group: "神经元生命周期", label: "重写变体" },
    PhaseInfo { value: PHASE_NEURON_RECYCLE, group: "神经元生命周期", label: "回收(容量)" },
    PhaseInfo { value: PHASE_NEURON_RECYCLE_RUNTIME, group: "神经元生命周期", label: "回收(runtime)" },
    // ── 神经元选择/补给 ──
    PhaseInfo { value: PHASE_SELECT_CANDIDATES, group: "神经元选择", label: "筛选候选" },
    PhaseInfo { value: PHASE_SELECT_CANDIDATES_DETAIL, group: "神经元选择", label: "筛选候选明细" },
    PhaseInfo { value: PHASE_SELECT_ASSISTANT_CANDIDATES, group: "神经元选择", label: "筛选助手候选" },
    PhaseInfo { value: PHASE_SELECT_ONE, group: "神经元选择", label: "选中单个" },
    PhaseInfo { value: PHASE_SELECT_ONE_LINK_BACK, group: "神经元选择", label: "选中回链" },
    PhaseInfo { value: PHASE_SELECT_NEURON_MODEL_INPUT, group: "神经元选择", label: "选型模型输入" },
    PhaseInfo { value: PHASE_SELECT_NEURON_MODEL_INPUT_FULL, group: "神经元选择", label: "选型模型输入(全)" },
    PhaseInfo { value: PHASE_SELECT_NEURON_MODEL_OUTPUT, group: "神经元选择", label: "选型模型输出" },
    PhaseInfo { value: PHASE_SELECT_NEURON_MODEL_DECISION, group: "神经元选择", label: "选型模型决策" },
    PhaseInfo { value: PHASE_SELECT_ROLE, group: "神经元选择", label: "角色选择" },
    PhaseInfo { value: PHASE_FILL_CANDIDATES_BATCH, group: "神经元选择", label: "批量补给候选" },
    PhaseInfo { value: PHASE_GENERATE_DRAFTS, group: "神经元选择", label: "生成草稿" },
    // ── 助手会话 / 轮询 ──
    PhaseInfo { value: PHASE_ASSISTANT_POLL_HANDLER, group: "助手会话", label: "轮询处理器" },
    PhaseInfo { value: PHASE_ASSISTANT_STEP, group: "助手会话", label: "会话步进" },
    PhaseInfo { value: PHASE_ASSISTANT_POLLER, group: "助手会话", label: "轮询器" },
    PhaseInfo { value: PHASE_ASSISTANT_CONVERSE, group: "助手会话", label: "会话对话" },
    PhaseInfo { value: PHASE_ASSISTANT_CONVERSE_STREAM, group: "助手会话", label: "会话对话(流式)" },
    PhaseInfo { value: PHASE_ASSISTANT_ERROR_RESIDENCY, group: "助手会话", label: "错误驻留" },
    PhaseInfo { value: PHASE_POLLER_TICK, group: "助手会话", label: "轮询心跳" },
    PhaseInfo { value: PHASE_POLLER_RUNTIME, group: "助手会话", label: "轮询运行时" },
    PhaseInfo { value: PHASE_POLLER_CONFIG, group: "助手会话", label: "轮询配置" },
    // ── 消息 / 模型调用 ──
    PhaseInfo { value: PHASE_SEND_MODEL_MESSAGE, group: "模型调用", label: "发送模型消息" },
    PhaseInfo { value: PHASE_SEND_MODEL_MESSAGE_STREAM, group: "模型调用", label: "发送模型消息(流式)" },
    PhaseInfo { value: PHASE_CALL_MODEL, group: "模型调用", label: "调用模型" },
    PhaseInfo { value: PHASE_CALL_JUDGEMENT, group: "模型调用", label: "调用裁决" },
    PhaseInfo { value: PHASE_RUN_JUDGEMENT_ROUND, group: "模型调用", label: "运行裁决轮" },
    PhaseInfo { value: PHASE_PARSE_TOOL_CALL, group: "模型调用", label: "解析工具调用" },
    PhaseInfo { value: PHASE_LLM_REQUEST_OUT, group: "模型调用", label: "模型请求体" },
    PhaseInfo { value: PHASE_LLM_RESPONSE_IN, group: "模型调用", label: "模型响应体" },
    PhaseInfo { value: PHASE_LLM_CALL_PERF, group: "模型调用", label: "模型调用耗时" },
    // ── 评分反馈 ──
    PhaseInfo { value: PHASE_APPLY_SCORE_FEEDBACK, group: "评分反馈", label: "应用评分反馈" },
    PhaseInfo { value: PHASE_MANUAL_SCORE_FEEDBACK, group: "评分反馈", label: "手动评分反馈" },
    PhaseInfo { value: PHASE_SCORE_FEEDBACK_COMMAND, group: "评分反馈", label: "评分反馈命令" },
    // ── 会话 / 协调 ──
    PhaseInfo { value: PHASE_SESSION_COORDINATOR, group: "会话管理", label: "会话协调器" },
    PhaseInfo { value: PHASE_STOP_SESSION, group: "会话管理", label: "停止会话" },
    PhaseInfo { value: PHASE_START_SESSION, group: "会话管理", label: "启动会话" },
    PhaseInfo { value: PHASE_TOOL_CONFIG, group: "会话管理", label: "工具配置" },
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