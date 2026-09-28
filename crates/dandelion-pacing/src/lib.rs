//! Integer token bucket on Tokio's monotonic clock, with cancellation.
use dandelion_core::{Error, Result};
use std::time::Duration;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
const SCALE: u128 = 1_000_000_000;

pub struct TokenBucket {
    rate: u64,
    capacity: u128,
    credits: u128,
    updated: Instant,
}
impl TokenBucket {
    pub fn new(bytes_per_second: u64, burst_bytes: u64) -> Result<Self> {
        if bytes_per_second == 0 || burst_bytes == 0 {
            return Err(Error::Config("rate and burst must be positive".into()));
        }
        let capacity = u128::from(burst_bytes) * SCALE;
        Ok(Self {
            rate: bytes_per_second,
            capacity,
            credits: capacity,
            updated: Instant::now(),
        })
    }
    pub async fn acquire(&mut self, bytes: u64, cancel: &CancellationToken) -> Result<()> {
        let need = u128::from(bytes) * SCALE;
        if need > self.capacity {
            return Err(Error::Config("packet exceeds token bucket burst".into()));
        }
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let now = Instant::now();
            let accrued = now
                .duration_since(self.updated)
                .as_nanos()
                .saturating_mul(u128::from(self.rate));
            self.credits = self.credits.saturating_add(accrued).min(self.capacity);
            self.updated = now;
            if self.credits >= need {
                self.credits -= need;
                return Ok(());
            }
            let nanos = (need - self.credits)
                .div_ceil(u128::from(self.rate))
                .min(u128::from(u64::MAX)) as u64;
            tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                _ = tokio::time::sleep(Duration::from_nanos(nanos)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn exact_rate_and_burst() {
        let mut bucket = TokenBucket::new(100, 100).unwrap();
        let cancel = CancellationToken::new();
        let start = Instant::now();
        bucket.acquire(100, &cancel).await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        bucket.acquire(50, &cancel).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(500));
        bucket.acquire(50, &cancel).await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(1));
        assert!(bucket.acquire(101, &cancel).await.is_err());
    }
    #[tokio::test(start_paused = true)]
    async fn cancellation_does_not_wait_for_tokens() {
        let mut bucket = TokenBucket::new(1, 100).unwrap();
        let cancel = CancellationToken::new();
        bucket.acquire(100, &cancel).await.unwrap();
        cancel.cancel();
        assert!(matches!(
            bucket.acquire(100, &cancel).await,
            Err(Error::Cancelled)
        ));
    }
}
