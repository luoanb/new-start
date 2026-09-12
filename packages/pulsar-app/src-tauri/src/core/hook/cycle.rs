//! 周期契约：**周期字段派生** + **可调项声明** + **调用前判定**。
//!
//! 设计口径（用户拍板，见 `docs/sdd-lab/2026-09-12_19-57_hook-cycle-management/`）：
//! - 周期 = **调度**（何时调用动作）；判定素材**只能**由调度方从 [`RoundContext`] 派生，
//!   业务状态（课题是否绑定等）不进周期条件，由动作内部自行判断。
//! - **一个概念 + 一个属性**：所有可调项都是「周期参数」（[`CycleParamSpec`]），
//!   [`CycleParamUsage`] 决定它用于「调用前判定」还是「动作内部读取」——不拆两套机制。
//! - 派生值**不写回、不落库**（与既有 `topic.extra` 计数并存，仅服务周期判定）。
//! - 本模块为**纯契约**：只依赖管线类型，不依赖任何业务模块。

use std::borrow::Cow;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::models::{ConversationMode, Message, MessageRole};
use crate::core::round_service::{RoundContext, RoundTriggerKind};

/// 周期字段：判定素材的唯一定义面。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CycleField {
    /// `ctx.mode`
    Mode,
    /// `ctx.trigger`
    Trigger,
    /// 本对话第几轮（`messages` 中 `role == Assistant` 的条数）
    RoundIndex,
    /// 用户介入轮次计数（`messages` 中 `role == User` 的条数）
    UserRounds,
    /// 距上次用户介入的推进轮次（最后一条 `User` 之后的 `Assistant` 条数）
    RoundsSinceUser,
    /// 轮次来源（用户轮 / 调度轮，由 `trigger` 派生）
    RoundOrigin,
    /// 产物形态（工具轮 / 收尾轮，可指定轮次）
    RoundShape,
}

/// 产物形态取哪一轮。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundRef {
    /// 本轮（仅 IP-3 及之后可得；IP-1 / IP-2 为 `None`）
    #[default]
    Current,
    /// 上一轮（任意注入点可得：`messages` 末条是否 `role == Tool`）
    Previous,
}

/// 轮次来源（正交维度 A）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundOrigin {
    UserRound,
    ScheduledRound,
}

/// 产物形态（正交维度 B）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundShape {
    ToolRound,
    SettlingRound,
}

impl RoundOrigin {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RoundOrigin::UserRound => "user_round",
            RoundOrigin::ScheduledRound => "scheduled_round",
        }
    }
}

impl RoundShape {
    pub const fn as_str(&self) -> &'static str {
        match self {
            RoundShape::ToolRound => "tool_round",
            RoundShape::SettlingRound => "settling_round",
        }
    }
}

/// 周期事实快照：每个注入点分发前派生一次（前序 hook 改写 `ctx` 后重新派生）。
///
/// 派生口径（见需求 §派生口径约束）：
/// - **不含本轮**：IP-1 / IP-2 时本轮产物尚未产生、用户消息可能未落库，故计数与
///   `round_shape_current` 反映「上一轮为止」；IP-1 的该偏移是原
///   `need_user_round_judgement` 首轮命中的前提。
/// - `round_shape_previous` 恒为「`messages` 末条是否 Tool 消息」；在 IP-5（本轮已落库）
///   它等价于本轮形态，判本轮请用 `round_shape_current`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CycleFacts {
    pub mode: String,
    pub trigger: String,
    pub round_index: u64,
    pub user_rounds: u64,
    pub rounds_since_user: u64,
    pub round_origin: RoundOrigin,
    pub round_shape_current: Option<RoundShape>,
    pub round_shape_previous: RoundShape,
}

impl CycleFacts {
    /// 从当前轮上下文派生（纯内存扫描，无 IO）。
    pub fn derive(ctx: &RoundContext) -> Self {
        let mut round_index: u64 = 0;
        let mut user_rounds: u64 = 0;
        let mut last_user_idx: Option<usize> = None;
        for (i, msg) in ctx.messages.iter().enumerate() {
            match msg.role {
                MessageRole::User => {
                    user_rounds += 1;
                    last_user_idx = Some(i);
                }
                MessageRole::Assistant => round_index += 1,
                _ => {}
            }
        }
        // 距上次用户介入的推进轮次：最后一条用户消息之后的 assistant 条数；
        // 从未有用户消息 → 全部轮次均为推进轮。
        let rounds_since_user = match last_user_idx {
            Some(idx) => ctx
                .messages
                .iter()
                .skip(idx + 1)
                .filter(|m| m.role == MessageRole::Assistant)
                .count() as u64,
            None => round_index,
        };
        let round_shape_previous = shape_of_previous(ctx.messages.last());
        let round_shape_current = ctx.outcome.as_ref().map(|p| {
            if p.tool_calls.is_some() || !p.tool_results.is_empty() {
                RoundShape::ToolRound
            } else {
                RoundShape::SettlingRound
            }
        });
        Self {
            mode: mode_str(&ctx.mode),
            trigger: trigger_str(ctx.trigger),
            round_index,
            user_rounds,
            rounds_since_user,
            round_origin: match ctx.trigger {
                RoundTriggerKind::User => RoundOrigin::UserRound,
                _ => RoundOrigin::ScheduledRound,
            },
            round_shape_current,
            round_shape_previous,
        }
    }

    /// 取字段的字符串值（枚举类字段）。
    pub fn field_str(&self, field: CycleField, round_ref: RoundRef) -> Option<Cow<'_, str>> {
        match field {
            CycleField::Mode => Some(Cow::Borrowed(self.mode.as_str())),
            CycleField::Trigger => Some(Cow::Borrowed(self.trigger.as_str())),
            CycleField::RoundOrigin => Some(Cow::Borrowed(self.round_origin.as_str())),
            CycleField::RoundShape => Some(Cow::Borrowed(self.shape(round_ref)?.as_str())),
            _ => None,
        }
    }

    /// 取字段的数值（计数类字段）。
    pub fn field_num(&self, field: CycleField) -> Option<i64> {
        match field {
            CycleField::RoundIndex => Some(self.round_index as i64),
            CycleField::UserRounds => Some(self.user_rounds as i64),
            CycleField::RoundsSinceUser => Some(self.rounds_since_user as i64),
            _ => None,
        }
    }

    /// 取字段的布尔值（布尔类字段）。
    pub fn field_bool(&self, field: CycleField, round_ref: RoundRef) -> Option<bool> {
        match field {
            CycleField::RoundShape => {
                Some(self.shape(round_ref)? == RoundShape::SettlingRound)
            }
            _ => None,
        }
    }

    fn shape(&self, round_ref: RoundRef) -> Option<RoundShape> {
        match round_ref {
            RoundRef::Current => self.round_shape_current,
            RoundRef::Previous => Some(self.round_shape_previous),
        }
    }
}

fn shape_of_previous(last: Option<&Message>) -> RoundShape {
    match last {
        Some(msg) if msg.role == MessageRole::Tool => RoundShape::ToolRound,
        _ => RoundShape::SettlingRound,
    }
}

fn mode_str(mode: &ConversationMode) -> String {
    match mode {
        ConversationMode::Chat => "chat",
        ConversationMode::Agent => "agent",
        ConversationMode::Assistant => "assistant",
        ConversationMode::System => "system",
    }
    .to_string()
}

fn trigger_str(trigger: RoundTriggerKind) -> String {
    match trigger {
        RoundTriggerKind::User => "user",
        RoundTriggerKind::ManualStep => "manual_step",
        RoundTriggerKind::Poller => "poller",
        RoundTriggerKind::AgentLoop => "agent_loop",
    }
    .to_string()
}

/// 可调项取值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CycleValue {
    Bool(bool),
    Int(i64),
    Str(String),
    Strs(Vec<String>),
}

/// 可调项形态：决定面板控件与校验方式。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycleParamKind {
    /// 枚举；`multi = true` → 多选（空集合视为「不限」）。
    Enum {
        values: &'static [&'static str],
        multi: bool,
    },
    Bool,
    /// 数值（含范围）：用于频率间隔等。
    Number { min: i64, max: i64 },
}

/// 可调项用途：框架调用前判定，还是动作内部读取。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleParamUsage {
    /// 框架在调用该动作前求值；不满足则**不调用**该动作。
    CallGate,
    /// 动作内部通过 `param_of` 读取（不影响是否调用）。
    Internal,
}

/// 可调项声明（由**动作提供方**声明；消费方只在其范围内取值）。
pub struct CycleParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    /// 绑定的周期字段；仅 `CallGate` 需要（`Internal` 为 `None`）。
    pub field: Option<CycleField>,
    /// `field == Some(RoundShape)` 时生效。
    pub round_ref: RoundRef,
    pub kind: CycleParamKind,
    pub default: CycleValue,
    pub usage: CycleParamUsage,
}

/// 校验取值是否符合声明（形态 / 候选值 / 范围）。
pub fn validate_value(spec: &CycleParamSpec, value: &CycleValue) -> Result<(), String> {
    let err = |msg: &str| Err(format!("param `{}`: {msg}", spec.key));
    match (&spec.kind, value) {
        (CycleParamKind::Bool, CycleValue::Bool(_)) => Ok(()),
        (CycleParamKind::Bool, _) => err("expect bool"),
        (CycleParamKind::Number { min, max }, CycleValue::Int(v)) => {
            if v < min || v > max {
                err(&format!("out of range {min}..={max}"))
            } else {
                Ok(())
            }
        }
        (CycleParamKind::Number { .. }, _) => err("expect int"),
        (CycleParamKind::Enum { values, multi: true }, CycleValue::Strs(selected)) => {
            match selected.iter().find(|s| !values.contains(&s.as_str())) {
                Some(bad) => err(&format!("unknown value `{bad}`")),
                None => Ok(()),
            }
        }
        (CycleParamKind::Enum { multi: true, .. }, _) => err("expect string list"),
        (CycleParamKind::Enum { values, multi: false }, CycleValue::Str(v)) => {
            if values.contains(&v.as_str()) {
                Ok(())
            } else {
                err(&format!("unknown value `{v}`"))
            }
        }
        (CycleParamKind::Enum { multi: false, .. }, _) => err("expect string"),
    }
}

/// 调用前判定：对所有 `CallGate` 项求「本次是否满足」。
///
/// - `Enum(multi)` → 派生字段值 ∈ 配置集合（**空集合 = 不限**）
/// - `Enum(single)` → 派生字段值 == 配置值
/// - `Bool` → 派生字段值 == 配置值
/// - `Number(N)` → 派生字段值 `% N == 0`
///
/// 缺失字段 / 取值缺失 / 形态不匹配 → **不匹配**（fail-safe，绝不误放行）。
pub fn gate_match(
    params: &[CycleParamSpec],
    values: &BTreeMap<&'static str, CycleValue>,
    facts: &CycleFacts,
) -> bool {
    gate_check(params, values, facts).is_none()
}

/// 同 [`gate_match`]，但返回**首个未通过的参数 key**（用于 skip 日志，可观测）。
pub fn gate_check(
    params: &[CycleParamSpec],
    values: &BTreeMap<&'static str, CycleValue>,
    facts: &CycleFacts,
) -> Option<&'static str> {
    for spec in params.iter().filter(|p| p.usage == CycleParamUsage::CallGate) {
        let Some(field) = spec.field else {
            continue;
        };
        let value = values.get(spec.key).unwrap_or(&spec.default);
        let ok = match (&spec.kind, value) {
            (CycleParamKind::Bool, CycleValue::Bool(expected)) => {
                facts.field_bool(field, spec.round_ref) == Some(*expected)
            }
            (CycleParamKind::Number { .. }, CycleValue::Int(step)) => {
                if *step <= 0 {
                    // N 非法（<=0）→ 视为「每轮」，与「不限」语义一致。
                    true
                } else {
                    facts
                        .field_num(field)
                        .is_some_and(|actual| actual % step == 0)
                }
            }
            (CycleParamKind::Enum { multi: true, .. }, CycleValue::Strs(selected)) => {
                if selected.is_empty() {
                    true
                } else {
                    facts
                        .field_str(field, spec.round_ref)
                        .is_some_and(|actual| selected.iter().any(|s| s == actual.as_ref()))
                }
            }
            (CycleParamKind::Enum { multi: false, .. }, CycleValue::Str(expected)) => {
                facts
                    .field_str(field, spec.round_ref)
                    .is_some_and(|actual| actual.as_ref() == expected.as_str())
            }
            _ => false,
        };
        if !ok {
            return Some(spec.key);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{ChatModelSelection, ConversationMode, MessageBody};
    use crate::core::round_types::SessionState;

    fn msg(role: MessageRole) -> Message {
        Message {
            role,
            body: MessageBody::Text {
                content: String::new(),
                reasoning: None,
                tool_calls: None,
            },
            timestamp: 0,
            neuron_id: None,
        }
    }

    fn ctx_with(messages: Vec<Message>, trigger: RoundTriggerKind) -> RoundContext {
        RoundContext {
            session_id: "s-1".into(),
            mode: ConversationMode::Assistant,
            seed: None,
            state: SessionState::default(),
            messages,
            model_input: String::new(),
            model: ChatModelSelection::new("p", "m"),
            tool_override: None,
            trigger,
            topic_id: None,
            reselect: true,
            nudge_persist: false,
            selected_neuron: None,
            outcome: None,
        }
    }

    #[test]
    fn derive_counts_and_origin() {
        // U A U A A：用户 2 次；轮次 3；最后一条用户之后 2 个 assistant
        let ctx = ctx_with(
            vec![
                msg(MessageRole::User),
                msg(MessageRole::Assistant),
                msg(MessageRole::User),
                msg(MessageRole::Assistant),
                msg(MessageRole::Assistant),
            ],
            RoundTriggerKind::Poller,
        );
        let facts = CycleFacts::derive(&ctx);
        assert_eq!(facts.user_rounds, 2);
        assert_eq!(facts.round_index, 3);
        assert_eq!(facts.rounds_since_user, 2);
        assert_eq!(facts.round_origin, RoundOrigin::ScheduledRound);
        assert_eq!(facts.mode, "assistant");
        assert_eq!(facts.trigger, "poller");
        // 无产物（IP-1/IP-2）→ 本轮形态未知；末条非 Tool → 上一轮为收尾轮
        assert_eq!(facts.round_shape_current, None);
        assert_eq!(facts.round_shape_previous, RoundShape::SettlingRound);
    }

    #[test]
    fn derive_without_any_user_message() {
        let ctx = ctx_with(
            vec![msg(MessageRole::Assistant), msg(MessageRole::Assistant)],
            RoundTriggerKind::Poller,
        );
        let facts = CycleFacts::derive(&ctx);
        assert_eq!(facts.user_rounds, 0);
        assert_eq!(facts.rounds_since_user, 2, "无用户消息 → 全部轮次为推进轮");
    }

    #[test]
    fn previous_shape_from_last_tool_message() {
        let ctx = ctx_with(
            vec![msg(MessageRole::Assistant), msg(MessageRole::Tool)],
            RoundTriggerKind::User,
        );
        let facts = CycleFacts::derive(&ctx);
        assert_eq!(facts.round_shape_previous, RoundShape::ToolRound);
        assert_eq!(facts.round_origin, RoundOrigin::UserRound);
    }

    #[test]
    fn field_accessors() {
        let ctx = ctx_with(
            vec![msg(MessageRole::User), msg(MessageRole::Assistant)],
            RoundTriggerKind::User,
        );
        let facts = CycleFacts::derive(&ctx);
        assert_eq!(
            facts.field_str(CycleField::RoundOrigin, RoundRef::Current),
            Some(Cow::Borrowed("user_round"))
        );
        assert_eq!(facts.field_num(CycleField::UserRounds), Some(1));
        assert_eq!(facts.field_num(CycleField::RoundIndex), Some(1));
        assert_eq!(facts.field_num(CycleField::Mode), None, "枚举字段无数值");
        // RoundShape/Current 缺失（本轮无产物）
        assert_eq!(facts.field_str(CycleField::RoundShape, RoundRef::Current), None);
        // RoundShape/Previous 可得
        assert_eq!(
            facts.field_bool(CycleField::RoundShape, RoundRef::Previous),
            Some(true),
            "上一轮非工具结束 → 收尾轮"
        );
    }

    fn spec(
        key: &'static str,
        field: CycleField,
        kind: CycleParamKind,
        default: CycleValue,
    ) -> CycleParamSpec {
        CycleParamSpec {
            key,
            label: "test",
            field: Some(field),
            round_ref: RoundRef::Current,
            kind,
            default,
            usage: CycleParamUsage::CallGate,
        }
    }

    fn values(list: Vec<(&'static str, CycleValue)>) -> BTreeMap<&'static str, CycleValue> {
        list.into_iter().collect()
    }

    #[test]
    fn gate_enum_multi_empty_means_unlimited() {
        let params = vec![spec(
            "mode",
            CycleField::Mode,
            CycleParamKind::Enum {
                values: &["assistant", "system"],
                multi: true,
            },
            CycleValue::Strs(vec![]),
        )];
        let facts = CycleFacts::derive(&ctx_with(vec![], RoundTriggerKind::User));
        assert!(gate_match(&params, &values(vec![]), &facts), "空集合 = 不限");
        assert!(gate_match(
            &params,
            &values(vec![("mode", CycleValue::Strs(vec!["assistant".into()]))]),
            &facts
        ));
        assert!(!gate_match(
            &params,
            &values(vec![("mode", CycleValue::Strs(vec!["chat".into()]))]),
            &facts
        ));
    }

    #[test]
    fn gate_number_is_modulo() {
        let params = vec![spec(
            "every",
            CycleField::UserRounds,
            CycleParamKind::Number { min: 1, max: 50 },
            CycleValue::Int(3),
        )];
        let mk = |user_rounds: usize| {
            let mut msgs = Vec::new();
            for _ in 0..user_rounds {
                msgs.push(msg(MessageRole::User));
            }
            CycleFacts::derive(&ctx_with(msgs, RoundTriggerKind::User))
        };
        assert!(gate_match(&params, &values(vec![]), &mk(0)), "0 % 3 == 0 覆盖首轮");
        assert!(gate_match(&params, &values(vec![]), &mk(3)));
        assert!(!gate_match(&params, &values(vec![]), &mk(1)));
        assert!(!gate_match(&params, &values(vec![]), &mk(2)));
    }

    #[test]
    fn gate_shape_current_missing_fails_safe() {
        let params = vec![spec(
            "shape",
            CycleField::RoundShape,
            CycleParamKind::Enum {
                values: &["tool_round", "settling_round"],
                multi: false,
            },
            CycleValue::Str("settling_round".into()),
        )];
        // IP-1/IP-2：本轮产物未知 → 不匹配（不误放行）
        let facts = CycleFacts::derive(&ctx_with(vec![], RoundTriggerKind::User));
        assert!(!gate_match(&params, &values(vec![]), &facts));
    }

    #[test]
    fn gate_internal_params_are_ignored() {
        let params = vec![CycleParamSpec {
            key: "brief_every",
            label: "test",
            field: None,
            round_ref: RoundRef::Current,
            kind: CycleParamKind::Number { min: 1, max: 50 },
            default: CycleValue::Int(3),
            usage: CycleParamUsage::Internal,
        }];
        let facts = CycleFacts::derive(&ctx_with(vec![], RoundTriggerKind::User));
        assert!(gate_match(&params, &values(vec![]), &facts));
    }

    #[test]
    fn validate_value_rules() {
        let multi = spec(
            "mode",
            CycleField::Mode,
            CycleParamKind::Enum {
                values: &["assistant", "system"],
                multi: true,
            },
            CycleValue::Strs(vec![]),
        );
        assert!(validate_value(&multi, &CycleValue::Strs(vec!["assistant".into()])).is_ok());
        assert!(validate_value(&multi, &CycleValue::Strs(vec!["chat".into()])).is_err());
        assert!(validate_value(&multi, &CycleValue::Int(1)).is_err());

        let num = spec(
            "every",
            CycleField::UserRounds,
            CycleParamKind::Number { min: 1, max: 50 },
            CycleValue::Int(3),
        );
        assert!(validate_value(&num, &CycleValue::Int(1)).is_ok());
        assert!(validate_value(&num, &CycleValue::Int(51)).is_err());
        assert!(validate_value(&num, &CycleValue::Bool(true)).is_err());

        let single = spec(
            "shape",
            CycleField::RoundShape,
            CycleParamKind::Enum {
                values: &["tool_round", "settling_round"],
                multi: false,
            },
            CycleValue::Str("settling_round".into()),
        );
        assert!(validate_value(&single, &CycleValue::Str("tool_round".into())).is_ok());
        assert!(validate_value(&single, &CycleValue::Str("nope".into())).is_err());

        let boolean = spec("switch", CycleField::RoundShape, CycleParamKind::Bool, CycleValue::Bool(true));
        assert!(validate_value(&boolean, &CycleValue::Bool(false)).is_ok());
        assert!(validate_value(&boolean, &CycleValue::Int(0)).is_err());
    }
}
