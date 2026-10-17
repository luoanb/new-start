//! 通用时钟：Unix 毫秒时间戳。
//!
//! 无业务语义，供核心与各扩展目录共享。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 毫秒时间戳。
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_millis()
}
