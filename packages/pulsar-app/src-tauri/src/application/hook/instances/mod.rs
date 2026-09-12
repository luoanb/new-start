//! 裁决定义：**一个 hook 一个文件**，内聚「常量 + schema + fallback + SPEC + run」。
//!
//! - 装配中的定义（进 `judgement::JUDGEMENT_SPECS`）：合并裁决 `user_round_judgement`（IP-1）
//!   与合并复盘 `round_review`（IP-5）；两者各有 `register` 装配入口，把定义注册进核心
//!   `HookRegistry`（注册默认关闭，开启由装配期显式设置）。
//! - 休眠定义（**定义·未注册**）：旧 4 条裁决 `score_feedback` / `match_topic` /
//!   `revise_topic` / `complete_scope`，源码与 inserts 契约完整保留，但不进定义清单、
//!   不注册、不开启（与神经元「惰性弃用」同一哲学）。

pub mod round_review;
pub mod user_round_judgement;

// 休眠定义：只有 SPEC + run，无消费者（不进定义清单 / 不注册 / 不开启）。
#[allow(dead_code)]
pub mod complete_scope;
#[allow(dead_code)]
pub mod match_topic;
#[allow(dead_code)]
pub mod revise_topic;
#[allow(dead_code)]
pub mod score_feedback;

pub use complete_scope::SYSTEM_TYPE_COMPLETE_SCOPE;
pub use match_topic::SYSTEM_TYPE_MATCH_TOPIC;
pub use revise_topic::SYSTEM_TYPE_REVISE_TOPIC;
pub use round_review::SYSTEM_TYPE_ROUND_REVIEW;
pub use score_feedback::SYSTEM_TYPE_SCORE_FEEDBACK;
pub use user_round_judgement::SYSTEM_TYPE_USER_ROUND_JUDGEMENT;
