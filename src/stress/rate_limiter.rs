// 全局 QPS 令牌桶限速器
//
// 多个 worker 共享一个 Arc<TokenBucket>，每次发包前 acquire 一个令牌。
// 令牌按 refill_rate (令牌/秒) 持续补充，容量上限 = refill_rate(即 1 秒的量)。
// 当无令牌可用时 acquire 会 sleep 到下一个令牌可用，从而实现全局 QPS 上限。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub struct TokenBucket {
    /// 无限速模式标志: true 时 acquire() 走无锁快速路径,完全绕过 Mutex。
    /// 解决高并发(≥10000 worker)下 unbounded 模式仍串行化在 Mutex 上的瓶颈。
    unbounded: AtomicBool,
    inner: Mutex<Inner>,
}

struct Inner {
    tokens: f64,
    capacity: f64,
    /// 每秒补充的令牌数
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    /// 构造一个不限速的令牌桶。
    /// acquire() 走原子快速路径,零锁开销。
    pub fn unbounded() -> Self {
        Self {
            unbounded: AtomicBool::new(true),
            inner: Mutex::new(Inner {
                tokens: f64::MAX,
                capacity: f64::MAX,
                refill_rate: f64::MAX,
                last_refill: Instant::now(),
            }),
        }
    }

    /// 构造限速令牌桶: qps = 每秒允许的请求数，容量 = qps(允许 1 秒突发)
    pub fn new(qps: u32) -> Self {
        let rate = qps as f64;
        Self {
            unbounded: AtomicBool::new(false),
            inner: Mutex::new(Inner {
                tokens: rate,
                capacity: rate,
                refill_rate: rate,
                last_refill: Instant::now(),
            }),
        }
    }

    /// 获取一个令牌。若需等待则 sleep。
    pub async fn acquire(&self) {
        // 快速路径: 无限速模式直接返回,避免 20000 worker 串行化在 Mutex 上
        if self.unbounded.load(Ordering::Relaxed) {
            return;
        }
        loop {
            let wait = {
                let mut inner = self.inner.lock().expect("令牌桶锁中毒");
                Self::refill(&mut inner);
                if inner.tokens >= 1.0 {
                    inner.tokens -= 1.0;
                    return;
                }
                // 计算获得 1 个令牌所需等待时间
                let needed = 1.0 - inner.tokens;
                Duration::from_secs_f64(needed / inner.refill_rate)
            };
            tokio::time::sleep(wait).await;
        }
    }

    /// 按时间流逝补充令牌
    fn refill(inner: &mut Inner) {
        let now = Instant::now();
        let elapsed = now.duration_since(inner.last_refill).as_secs_f64();
        inner.last_refill = now;
        inner.tokens = (inner.tokens + elapsed * inner.refill_rate).min(inner.capacity);
    }

    /// 当前可用令牌数(主要用于测试/调试)
    #[allow(dead_code)]
    pub fn available_tokens(&self) -> f64 {
        let mut inner = self.inner.lock().expect("令牌桶锁中毒");
        Self::refill(&mut inner);
        inner.tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_unbounded_never_blocks() {
        let bucket = TokenBucket::unbounded();
        let available = bucket.available_tokens();
        assert!(available.is_finite() == false || available >= 1.0);
    }

    #[tokio::test]
    async fn test_unbounded_acquire_returns_immediately() {
        let bucket = Arc::new(TokenBucket::unbounded());
        let start = Instant::now();
        for _ in 0..1000 {
            bucket.acquire().await;
        }
        // 1000 次不应耗时超过 1 秒
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn test_capacity_equals_qps() {
        let bucket = TokenBucket::new(100);
        // 初始令牌 = qps (允许 1 秒突发)
        let available = bucket.available_tokens();
        assert!(
            (available - 100.0).abs() < 1.0,
            "初始应有约 100 令牌, 实际 {}",
            available
        );
    }

    #[tokio::test]
    async fn test_acquire_decrements_tokens() {
        let bucket = TokenBucket::new(100);
        bucket.acquire().await;
        let available = bucket.available_tokens();
        assert!(
            (available - 99.0).abs() < 1.0,
            "取 1 个后应剩约 99, 实际 {}",
            available
        );
    }

    #[tokio::test]
    async fn test_rate_limiting_enforced() {
        // qps=20，容量=20(先用完突发量)，再取应等待约 50ms
        let bucket = Arc::new(TokenBucket::new(20));
        // 用完初始突发量
        for _ in 0..20 {
            bucket.acquire().await;
        }
        let start = Instant::now();
        bucket.acquire().await; // 第 21 个，需等待约 1/20s = 50ms
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(40),
            "应等待约 50ms, 实际 {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(200),
            "等待不应超过 200ms, 实际 {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_concurrent_acquires_share_bucket() {
        // 2 个并发 worker 共享 qps=40，共取 40 个，应耗时约 1 秒(突发用完后按速率)
        let bucket = Arc::new(TokenBucket::new(40));
        let b1 = bucket.clone();
        let b2 = bucket.clone();
        let h1 = tokio::spawn(async move {
            for _ in 0..20 {
                b1.acquire().await;
            }
        });
        let h2 = tokio::spawn(async move {
            for _ in 0..20 {
                b2.acquire().await;
            }
        });
        let start = Instant::now();
        h1.await.unwrap();
        h2.await.unwrap();
        // 突发 40 个用完即结束，不应超 1 秒
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
