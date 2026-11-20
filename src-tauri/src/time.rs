//! 时间工具函数（统一使用 Unix 毫秒，避免引入额外日期库）。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前时间戳（Unix 毫秒）。
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// 当前时间戳（Unix 秒）。
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
