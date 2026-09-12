//! 通用时钟：Unix 毫秒时间戳。
//!
//! 无业务语义，供核心（消息时间戳 / 会话 `updated_at`）与各扩展目录共享。
//! 原 `core::conversation_store::now_ms`（存储实现迁往 `stores/` 前的去扩展依赖）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 毫秒时间戳。
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis()
}
