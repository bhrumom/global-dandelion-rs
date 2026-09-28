//! The shared, application-owned meter sits BELOW TLS. Ciphertext handshake,
//! data, acknowledgements and retries are all limited; IP/TCP headers are not.
use dandelion_core::{Error, Result};
use std::{
    future::Future,
    io,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep},
};
const SCALE: u128 = 1_000_000_000;
const EXHAUSTED: &str = "dandelion ciphertext budget exhausted";
struct State {
    charged: u64,
    accepted: u64,
    received: u64,
    limit: u64,
    rate: u64,
    capacity: u128,
    credits: u128,
    updated: Instant,
    schedule: Option<dandelion_core::schedule::DayNight>,
}
#[derive(Clone)]
pub struct WireBudget(Arc<Mutex<State>>);
impl WireBudget {
    pub fn new(limit: u64, rate: u64, burst: u64) -> Result<Self> {
        if limit == 0 || rate == 0 || burst == 0 {
            return Err(Error::Config(
                "wire budget/rate/burst must be positive".into(),
            ));
        }
        Ok(Self(Arc::new(Mutex::new(State {
            charged: 0,
            accepted: 0,
            received: 0,
            limit,
            rate,
            capacity: u128::from(burst) * SCALE,
            credits: u128::from(burst) * SCALE,
            updated: Instant::now(),
            schedule: None,
        }))))
    }
    pub fn set_schedule(&self, schedule: Option<dandelion_core::schedule::DayNight>) -> Result<()> {
        if let Some(value) = &schedule {
            value.validate()?;
        }
        self.0
            .lock()
            .map_err(|_| Error::Task("wire meter poisoned".into()))?
            .schedule = schedule;
        Ok(())
    }
    pub fn totals(&self) -> Result<(u64, u64, u64)> {
        let s = self
            .0
            .lock()
            .map_err(|_| Error::Task("wire meter lock poisoned".into()))?;
        Ok((s.charged, s.accepted, s.received))
    }
    pub fn set_rate(&self, rate: u64) -> Result<()> {
        if rate == 0 {
            return Err(Error::Config("wire rate cannot be zero".into()));
        }
        let mut s = self
            .0
            .lock()
            .map_err(|_| Error::Task("wire meter lock poisoned".into()))?;
        refill(&mut s);
        s.rate = rate;
        Ok(())
    }
}
fn refill(s: &mut State) {
    let now = Instant::now();
    s.credits = s
        .credits
        .saturating_add(
            now.duration_since(s.updated)
                .as_nanos()
                .saturating_mul(u128::from(s.rate)),
        )
        .min(s.capacity);
    s.updated = now;
    if let Some(schedule) = &s.schedule
        && let Ok(time) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
    {
        s.rate = schedule.rate_at(time.as_secs().min(i64::MAX as u64) as i64);
    }
}
pub fn classify(error: io::Error) -> Error {
    if error.to_string().contains(EXHAUSTED) {
        Error::BudgetExhausted
    } else {
        Error::Io(error)
    }
}
pub struct Metered<T> {
    inner: T,
    budget: WireBudget,
    sleep: Option<Pin<Box<Sleep>>>,
    local_accepted: u64,
}
impl<T> Metered<T> {
    pub fn accepted_bytes(&self) -> u64 {
        self.local_accepted
    }

    pub fn new(inner: T, budget: WireBudget) -> Self {
        Self {
            inner,
            budget,
            sleep: None,
            local_accepted: 0,
        }
    }
}
impl<T: AsyncWrite + Unpin> AsyncWrite for Metered<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if buf.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if let Some(sleep) = &mut this.sleep {
            if sleep.as_mut().poll(cx).is_pending() {
                return Poll::Pending;
            }
            this.sleep = None;
        }
        let mut state = match this.budget.0.lock() {
            Ok(s) => s,
            Err(_) => return Poll::Ready(Err(io::Error::other("wire meter poisoned"))),
        };
        refill(&mut state);
        let remaining = state.limit - state.charged;
        if remaining == 0 {
            return Poll::Ready(Err(io::Error::other(EXHAUSTED)));
        }
        let available = (state.credits / SCALE)
            .min(u128::from(remaining))
            .min(buf.len() as u128) as usize;
        if available == 0 {
            let delay = (SCALE - state.credits)
                .div_ceil(u128::from(state.rate))
                .min(u128::from(u64::MAX)) as u64;
            drop(state);
            this.sleep = Some(Box::pin(tokio::time::sleep(Duration::from_nanos(
                delay.min(1_000_000_000),
            ))));
            if let Some(sleep) = &mut this.sleep {
                let _ = sleep.as_mut().poll(cx);
            }
            return Poll::Pending;
        }
        match Pin::new(&mut this.inner).poll_write(cx, &buf[..available]) {
            Poll::Ready(Ok(n)) => {
                state.charged += n as u64;
                state.accepted += n as u64;
                this.local_accepted += n as u64;
                state.credits -= n as u128 * SCALE;
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => {
                state.charged += available as u64;
                state.credits -= available as u128 * SCALE;
                Poll::Ready(Err(e))
            }
            Poll::Pending => Poll::Pending,
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
impl<T: AsyncRead + Unpin> AsyncRead for Metered<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut state = match this.budget.0.lock() {
            Ok(s) => s,
            Err(_) => return Poll::Ready(Err(io::Error::other("wire meter poisoned"))),
        };
        let left = state.limit - state.received;
        if left == 0 {
            return Poll::Ready(Err(io::Error::other(EXHAUSTED)));
        }
        let amount = left.min(buf.remaining() as u64) as usize;
        let (result, read) = {
            let mut sub = ReadBuf::new(buf.initialize_unfilled_to(amount));
            let result = Pin::new(&mut this.inner).poll_read(cx, &mut sub);
            (result, sub.filled().len())
        };
        buf.advance(read);
        state.received += read as u64;
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn actual_writes_are_capped_across_shared_streams() {
        let budget = WireBudget::new(5, 1000, 1000).unwrap();
        let (a, mut b) = tokio::io::duplex(64);
        let mut metered = Metered::new(a, budget.clone());
        metered.write_all(b"12345").await.unwrap();
        assert!(matches!(
            classify(metered.write_all(b"x").await.unwrap_err()),
            Error::BudgetExhausted
        ));
        let mut buf = [0; 5];
        b.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"12345");
        assert_eq!(budget.totals().unwrap().0, 5);
    }
    #[tokio::test(start_paused = true)]
    async fn rate_limit_is_monotonic() {
        let budget = WireBudget::new(100, 10, 10).unwrap();
        let (a, _b) = tokio::io::duplex(128);
        let mut io = Metered::new(a, budget);
        let start = Instant::now();
        io.write_all(&[0; 20]).await.unwrap();
        assert!(start.elapsed() >= Duration::from_secs(1));
    }
}
