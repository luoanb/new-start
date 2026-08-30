//! Hook 注册表：`HookInstance`（定义 + 执行单元）+ `ACTIVE_HOOKS`（启用清单）
//! + `LEGACY_HOOKS`（休眠清单）。
//!
//! - **注册式管理**：hook 的调整 = 实例在两张清单间移动，代码与 inserts 契约保留原位
//!   （与神经元「惰性弃用」同一哲学；`LEGACY_HOOKS` 仅存档，不执行、不进面板）。
//! - **执行分发**：编排层（`assistant_session::AssistantHooks::round_before/after`）
//!   按 `def.inject_point` 遍历 `ACTIVE_HOOKS` 调用 `run`；门控语义下沉在各实例 run 内。
//! - **定义查询**：`hook::judgement::hook_def()` 只查 `ACTIVE_HOOKS`（裁决执行与
//!   面板语义不变；legacy system_type 在账本中按未知类型回退展示）。

use crate::core::assistant_session::AssistantHooks;
use crate::core::conversation_runner::RoundContext;
use crate::core::error::AppResult;

use super::defs::{BoxFuture, InjectPointId};
use super::instances;
use super::judgement::HookDef;

/// 执行入口：按注入点读写权限分两类（IP-1 可改写 ctx / IP-5 只读，副作用自办）。
pub(crate) enum HookRun {
    /// IP-1 AfterLoadContext：可在 run 内改写 ctx（课题路由切换会话 / 绑定 / 计数推进）。
    Before(
        for<'a> fn(&'a AssistantHooks<'a>, &'a mut RoundContext) -> BoxFuture<'a, AppResult<()>>,
    ),
    /// IP-5 AfterPersistOutcome：产物已落库，只读整轮上下文。
    After(
        for<'a> fn(&'a AssistantHooks<'a>, &'a RoundContext) -> BoxFuture<'a, AppResult<()>>,
    ),
}

/// 单个 hook 实例：定义（`HookDef` 规则）+ 执行（`run` 实现），就近内聚在同一文件。
pub(crate) struct HookInstance {
    pub(crate) def: HookDef,
    pub(crate) run: HookRun,
}

/// 启用中的 hook 实例（执行与面板数据源；顺序即同注入点内的执行顺序）。
pub(crate) static ACTIVE_HOOKS: &[&HookInstance] = &[
    &instances::user_round_judgement::INSTANCE,
    &instances::round_review::INSTANCE,
];

/// 休眠 hook 实例（旧 4 条裁决：代码保留、不注册执行；回切 = 移入 `ACTIVE_HOOKS`）。
/// lib 构建下仅作存档无消费者（测试断言其完整性），`allow(dead_code)` 抑制休眠警告。
#[allow(dead_code)]
pub(crate) static LEGACY_HOOKS: &[&HookInstance] = &[
    &instances::score_feedback::INSTANCE,
    &instances::match_topic::INSTANCE,
    &instances::revise_topic::INSTANCE,
    &instances::complete_scope::INSTANCE,
];

/// 按注入点取启用实例（保持清单声明顺序；编排层按此遍历执行）。
pub fn active_hooks_at(point: InjectPointId) -> impl Iterator<Item = &'static HookInstance> {
    ACTIVE_HOOKS
        .iter()
        .copied()
        .filter(move |h| h.def.inject_point == point.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_hooks_exactly_merged_two() {
        assert_eq!(ACTIVE_HOOKS.len(), 2);
        assert_eq!(
            ACTIVE_HOOKS[0].def.system_type,
            instances::SYSTEM_TYPE_USER_ROUND_JUDGEMENT
        );
        assert_eq!(
            ACTIVE_HOOKS[1].def.system_type,
            instances::SYSTEM_TYPE_ROUND_REVIEW
        );
    }

    #[test]
    fn legacy_hooks_exactly_old_four() {
        assert_eq!(LEGACY_HOOKS.len(), 4);
        assert_eq!(
            LEGACY_HOOKS[0].def.system_type,
            instances::SYSTEM_TYPE_SCORE_FEEDBACK
        );
        assert_eq!(
            LEGACY_HOOKS[1].def.system_type,
            instances::SYSTEM_TYPE_MATCH_TOPIC
        );
        assert_eq!(
            LEGACY_HOOKS[2].def.system_type,
            instances::SYSTEM_TYPE_REVISE_TOPIC
        );
        assert_eq!(
            LEGACY_HOOKS[3].def.system_type,
            instances::SYSTEM_TYPE_COMPLETE_SCOPE
        );
    }

    #[test]
    fn system_types_unique_across_lists() {
        let mut types: Vec<_> = ACTIVE_HOOKS
            .iter()
            .chain(LEGACY_HOOKS)
            .map(|h| h.def.system_type)
            .collect();
        let total = types.len();
        types.sort_unstable();
        types.dedup();
        assert_eq!(types.len(), total, "system_type must be unique across lists");
    }

    #[test]
    fn active_hooks_at_filters_by_inject_point() {
        let ip1: Vec<_> = active_hooks_at(InjectPointId::AfterLoadContext).collect();
        assert_eq!(ip1.len(), 1);
        assert_eq!(
            ip1[0].def.system_type,
            instances::SYSTEM_TYPE_USER_ROUND_JUDGEMENT
        );
        let ip5: Vec<_> = active_hooks_at(InjectPointId::AfterPersistOutcome).collect();
        assert_eq!(ip5.len(), 1);
        assert_eq!(ip5[0].def.system_type, instances::SYSTEM_TYPE_ROUND_REVIEW);
        assert!(active_hooks_at(InjectPointId::AfterCallModel).next().is_none());
    }

    #[test]
    fn active_instance_finds_active_only() {
        // hook_def（judgement 层）只查 ACTIVE_HOOKS；legacy 实例不可经启用清单查到。
        assert!(ACTIVE_HOOKS
            .iter()
            .all(|h| h.def.system_type != instances::SYSTEM_TYPE_COMPLETE_SCOPE));
    }

    #[test]
    fn run_variant_matches_inject_point() {
        // Before ↔ AfterLoadContext、After ↔ AfterPersistOutcome 一一对应。
        for h in ACTIVE_HOOKS.iter().chain(LEGACY_HOOKS.iter()) {
            match (&h.run, h.def.inject_point) {
                (HookRun::Before(_), p) => {
                    assert_eq!(p, InjectPointId::AfterLoadContext.as_str());
                }
                (HookRun::After(_), p) => {
                    assert_eq!(p, InjectPointId::AfterPersistOutcome.as_str());
                }
            }
        }
    }
}
