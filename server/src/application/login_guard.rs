//! 登录失败计数与指数退避（A4）。
//!
//! 单进程内存态，按「账号」与「来源 IP」两个维度分别计数；任一维度命中锁定即拒绝登录，
//! 登录成功则两个维度都清零。
//!
//! **为什么不用 DB**：登录风暴是**在线**问题，内存态足够且不引入写放大；重启即遗忘——
//! 可接受（重启本身重置了攻击者「已猜了多少次」的假设）。
//!
//! **可测性**：时间以参数注入（`now: Instant`），因此单测能精确控制「过了多久」，
//! 不需要 sleep、也不需要 mock 时钟。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// 免锁失败次数：前 2 次只记账不锁（正常手误不该被拦）。
pub const FREE_ATTEMPTS: u32 = 2;
/// 单次锁定等待上限（秒）。
pub const MAX_BACKOFF_SECS: u64 = 300;

/// 退避时长（纯函数）：第 `failures` 次失败后需等待的秒数。
///
/// 曲线：1-2 次 → 0s；3 次 → 1s；4 次 → 2s；… 指数增长并在 [`MAX_BACKOFF_SECS`] 封顶。
pub fn backoff_secs(failures: u32) -> u64 {
    if failures <= FREE_ATTEMPTS {
        return 0;
    }
    let exp = (failures - FREE_ATTEMPTS - 1).min(16);
    (1u64 << exp).min(MAX_BACKOFF_SECS)
}

#[derive(Default, Clone, Copy)]
struct Attempt {
    failures: u32,
    blocked_until_secs: u64,
}

/// 登录守卫：键为 `user:<name>` / `ip:<addr>`，两个维度独立计数。
#[derive(Default)]
pub struct LoginGuard {
    inner: Mutex<HashMap<String, Attempt>>,
    /// 注入式时钟原点（`Instant` 不实现 Default，这里用进程启动时刻）。
    epoch: Option<Instant>,
}

impl LoginGuard {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            epoch: Some(Instant::now()),
        }
    }

    /// 把秒数换算成「相对纪元」的时间点（测试传 0 纪元即可精确控制）。
    fn at(&self, now: Instant) -> u64 {
        now.duration_since(self.epoch.unwrap_or(now)).as_secs()
    }

    /// 该键当前是否被锁定；返回剩余等待秒数。
    pub fn blocked_for(&self, key: &str, now: Instant) -> Option<u64> {
        let t = self.at(now);
        let mut inner = self.inner.lock().unwrap();
        let a = inner.get(key).copied()?;
        if a.blocked_until_secs > t {
            return Some(a.blocked_until_secs - t);
        }
        // 锁定期已过：清除锁定标记（失败计数保留，连续失败会继续加重退避）
        if let Some(entry) = inner.get_mut(key) {
            entry.blocked_until_secs = 0;
        }
        None
    }

    /// 记一次失败：累加计数并按 [`backoff_secs`] 设置锁定窗口；返回当前失败次数。
    pub fn record_failure(&self, key: &str, now: Instant) -> u32 {
        let t = self.at(now);
        let mut inner = self.inner.lock().unwrap();
        let entry = inner.entry(key.to_string()).or_default();
        entry.failures = entry.failures.saturating_add(1);
        let wait = backoff_secs(entry.failures);
        if wait > 0 {
            entry.blocked_until_secs = t + wait;
        }
        entry.failures
    }

    /// 登录成功：清零该键的失败计数与锁定。
    pub fn record_success(&self, key: &str) {
        self.inner.lock().unwrap().remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn backoff_is_zero_until_threshold() {
        assert_eq!(backoff_secs(0), 0);
        assert_eq!(backoff_secs(1), 0);
        assert_eq!(backoff_secs(2), 0);
    }

    #[test]
    fn backoff_grows_exponentially_then_caps() {
        assert_eq!(backoff_secs(3), 1);
        assert_eq!(backoff_secs(4), 2);
        assert_eq!(backoff_secs(5), 4);
        assert_eq!(backoff_secs(6), 8);
        assert_eq!(backoff_secs(20), MAX_BACKOFF_SECS, "高位封顶");
        assert!(backoff_secs(9) <= MAX_BACKOFF_SECS);
    }

    #[test]
    fn first_two_failures_do_not_block() {
        let g = LoginGuard::new();
        let t0 = g.epoch.unwrap();
        assert_eq!(g.record_failure("user:a", t0), 1);
        assert!(g.blocked_for("user:a", t0).is_none(), "第 1 次不锁");
        assert_eq!(g.record_failure("user:a", t0), 2);
        assert!(g.blocked_for("user:a", t0).is_none(), "第 2 次不锁");
    }

    #[test]
    fn third_failure_blocks_for_one_second() {
        let g = LoginGuard::new();
        let t0 = g.epoch.unwrap();
        g.record_failure("user:a", t0);
        g.record_failure("user:a", t0);
        g.record_failure("user:a", t0);
        assert_eq!(g.blocked_for("user:a", t0), Some(1), "第 3 次锁 1 秒");
    }

    #[test]
    fn lock_expires_with_time() {
        let g = LoginGuard::new();
        let t0 = g.epoch.unwrap();
        for _ in 0..3 {
            g.record_failure("ip:1.2.3.4", t0);
        }
        assert!(g.blocked_for("ip:1.2.3.4", t0).is_some());
        // 1 秒后解锁，且失败计数保留（第 4 次失败会退避 2 秒）
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(g.blocked_for("ip:1.2.3.4", t1), None);
        assert_eq!(g.record_failure("ip:1.2.3.4", t1), 4);
        assert_eq!(g.blocked_for("ip:1.2.3.4", t1), Some(2));
    }

    #[test]
    fn success_clears_counter_and_lock() {
        let g = LoginGuard::new();
        let t0 = g.epoch.unwrap();
        for _ in 0..4 {
            g.record_failure("user:b", t0);
        }
        assert!(g.blocked_for("user:b", t0).is_some());
        g.record_success("user:b");
        assert!(g.blocked_for("user:b", t0).is_none(), "成功即解锁");
        assert_eq!(g.record_failure("user:b", t0), 1, "计数已清零");
    }

    #[test]
    fn keys_are_independent() {
        let g = LoginGuard::new();
        let t0 = g.epoch.unwrap();
        for _ in 0..3 {
            g.record_failure("user:a", t0);
        }
        assert!(g.blocked_for("user:a", t0).is_some());
        assert!(
            g.blocked_for("user:other", t0).is_none(),
            "不同账号互不影响"
        );
    }
}
