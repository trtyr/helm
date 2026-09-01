//! 在线/离线判定：心跳超时纯函数（无 IO，可单测）。

use chrono::{DateTime, Duration, Utc};

/// 心跳是否已超时。
///
/// - `last_seen` 为 `None`（从未上报过心跳）→ 视为超时。
/// - `now - last_seen > timeout` → 超时。
pub fn is_stale(last_seen: Option<DateTime<Utc>>, now: DateTime<Utc>, timeout: Duration) -> bool {
    match last_seen {
        None => true,
        Some(t) => now.signed_duration_since(t) > timeout,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn never_seen_is_stale() {
        assert!(is_stale(None, Utc::now(), Duration::seconds(30)));
    }

    #[test]
    fn fresh_heartbeat_is_not_stale() {
        let now = Utc::now();
        let last = now - Duration::seconds(10);
        assert!(!is_stale(Some(last), now, Duration::seconds(30)));
    }

    #[test]
    fn expired_heartbeat_is_stale() {
        let now = Utc::now();
        let last = now - Duration::seconds(31);
        assert!(is_stale(Some(last), now, Duration::seconds(30)));
    }
}
