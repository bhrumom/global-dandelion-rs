//! Pure monotonic rate estimation. No clocks, tasks or global state are hidden.
use std::time::Duration;
#[derive(Default, Debug)]
pub struct RateEstimator {
    previous_bytes: u64,
    previous_time: Duration,
}
impl RateEstimator {
    pub fn sample(&mut self, bytes: u64, time: Duration) -> u64 {
        let elapsed = time.saturating_sub(self.previous_time);
        let delta = bytes.saturating_sub(self.previous_bytes);
        self.previous_bytes = bytes;
        self.previous_time = time;
        if elapsed.is_zero() {
            0
        } else {
            (u128::from(delta) * 1_000_000_000 / elapsed.as_nanos()).min(u128::from(u64::MAX))
                as u64
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_reset_and_monotonic_rates() {
        let mut m = RateEstimator::default();
        assert_eq!(m.sample(100, Duration::from_millis(500)), 200);
        assert_eq!(m.sample(100, Duration::from_secs(1)), 0);
        assert_eq!(m.sample(1, Duration::from_secs(2)), 0);
        assert_eq!(m.sample(101, Duration::from_secs(3)), 100);
        assert_eq!(m.sample(102, Duration::from_secs(3)), 0);
    }
}
