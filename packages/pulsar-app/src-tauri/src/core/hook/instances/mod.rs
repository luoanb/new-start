//! 裁决 hook 实例：**一个 hook 一个文件**，内聚「常量 + schema + fallback + INSTANCE + run」。
//!
//! - 活跃实例（`registry::ACTIVE_HOOKS`）：合并裁决 `user_round_judgement`（IP-1）与
//!   合并复盘 `round_review`（IP-5）。
//! - 休眠实例（`registry::LEGACY_HOOKS`）：旧 4 条裁决 `score_feedback` / `match_topic` /
//!   `revise_topic` / `complete_scope`，代码与 inserts 契约完整保留、只是不注册执行
//!   （与神经元「惰性弃用」同一哲学）；回切 = 实例从 `LEGACY_HOOKS` 移入 `ACTIVE_HOOKS`。

pub mod complete_scope;
pub mod match_topic;
pub mod revise_topic;
pub mod round_review;
pub mod score_feedback;
pub mod user_round_judgement;

pub use complete_scope::SYSTEM_TYPE_COMPLETE_SCOPE;
pub use match_topic::SYSTEM_TYPE_MATCH_TOPIC;
pub use revise_topic::SYSTEM_TYPE_REVISE_TOPIC;
pub use round_review::SYSTEM_TYPE_ROUND_REVIEW;
pub use score_feedback::SYSTEM_TYPE_SCORE_FEEDBACK;
pub use user_round_judgement::SYSTEM_TYPE_USER_ROUND_JUDGEMENT;
