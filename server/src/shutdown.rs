//! 优雅停机（T006）：SIGTERM / SIGINT → 停接新活 → 等在飞收尾 → 退出。
//!
//! **为什么需要**：此前服务端没有停机信号处理，SIGTERM 直接把进程砍掉——在飞的 job
//! 会一直停在 `running`，直到 `HELM_JOB_TIMEOUT_SECS`（默认 300s）被 sweeper 标成
//! `timed_out`（**假超时**：命令其实早在进程消失时就没了）。T006 让停机可控；
//! 上一进程遗留状态的对账见 T007。
//!
//! **结构**：一个信号源（`watch`）多个消费者——HTTP(axum) 与全部 gRPC 监听器共用同一
//! 信号，两个面同时进入 drain。`DRAIN_TIMEOUT` 是兜底：SSE / 终端 WebSocket 这类长连接
//! 不肯收尾时，也不能让进程被无限钉住。

use std::time::Duration;

use tokio::sync::watch;

/// 触发停机后，允许在飞请求与流收尾的时间上限。超过则由兜底路径强制结束 serve。
pub const DRAIN_TIMEOUT: Duration = Duration::from_secs(15);

/// 停机协调器。Clone 便宜：只共享一个 watch 接收端。
#[derive(Clone)]
pub struct Shutdown {
    rx: watch::Receiver<bool>,
}

impl Shutdown {
    /// 等待停机信号；若已被触发过则立即返回。
    pub async fn wait(&self) {
        let mut rx = self.rx.clone();
        loop {
            if *rx.borrow() {
                return;
            }
            // 发送端随进程存续；若意外 drop，按「已停机」处理——fail 到退出，而不是卡住
            if rx.changed().await.is_err() {
                return;
            }
        }
    }

    /// 兜底时限：停机信号触发后再等 [`DRAIN_TIMEOUT`]。用于给 serve 的排空加边界。
    pub async fn drain_deadline(&self) {
        self.wait().await;
        tokio::time::sleep(DRAIN_TIMEOUT).await;
    }
}

/// 注册信号处理器并返回协调器（进程内调用一次）。
pub fn init() -> Shutdown {
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        let reason = wait_for_signal().await;
        tracing::info!(
            signal = reason,
            drain_timeout_secs = DRAIN_TIMEOUT.as_secs(),
            "收到停机信号：停接新连接，等待在飞请求与流收尾"
        );
        let _ = tx.send(true);
    });
    Shutdown { rx }
}

/// 测试用构造：直接注入 watch 接收端（生产路径走 [`init`]）。
pub fn from_receiver(rx: watch::Receiver<bool>) -> Shutdown {
    Shutdown { rx }
}

/// 等待 SIGTERM 或 Ctrl-C（SIGINT）。
#[cfg(unix)]
async fn wait_for_signal() -> &'static str {
    use tokio::signal::unix::{SignalKind, signal};

    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            // 注册失败只降级，不阻断启动：Ctrl-C 仍然可用
            tracing::error!(error = %e, "SIGTERM 处理器注册失败，本次运行只响应 Ctrl-C");
            let _ = tokio::signal::ctrl_c().await;
            return "SIGINT";
        }
    };
    tokio::select! {
        _ = term.recv() => "SIGTERM",
        _ = tokio::signal::ctrl_c() => "SIGINT",
    }
}

/// 非 unix 平台：只有 Ctrl-C。
#[cfg(not(unix))]
async fn wait_for_signal() -> &'static str {
    let _ = tokio::signal::ctrl_c().await;
    "SIGINT"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn wait_returns_after_trigger() {
        let (tx, rx) = watch::channel(false);
        let shutdown = from_receiver(rx);
        tokio::spawn(async move {
            let _ = tx.send(true);
        });
        tokio::time::timeout(Duration::from_secs(2), shutdown.wait())
            .await
            .expect("信号触发后 wait 必须返回");
    }

    #[tokio::test]
    async fn wait_returns_immediately_if_already_triggered() {
        let (tx, rx) = watch::channel(false);
        tx.send(true).expect("首次发送必成功");
        let shutdown = from_receiver(rx);
        tokio::time::timeout(Duration::from_millis(200), shutdown.wait())
            .await
            .expect("已触发状态下 wait 必须立即返回");
    }

    #[tokio::test]
    async fn wait_survives_sender_drop() {
        // 发送端消失（进程收尾场景）：按「已停机」处理，不能把调用方永久挂住
        let (tx, rx) = watch::channel(false);
        let shutdown = from_receiver(rx);
        drop(tx);
        tokio::time::timeout(Duration::from_millis(200), shutdown.wait())
            .await
            .expect("发送端 drop 后 wait 必须返回");
    }
}
