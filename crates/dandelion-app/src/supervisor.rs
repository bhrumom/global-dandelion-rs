use dandelion_config::ValidatedConfig;
use dandelion_core::{
    Chunk, Error, MAX_DATAGRAM_BYTES, Result, RunStatus, Snapshot, TrafficBudget,
};
use dandelion_pacing::TokenBucket;
use dandelion_scanner::ScanOptions;
use dandelion_transport::{Loopback, Transport};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct Supervisor {
    config: ValidatedConfig,
    observer: Option<tokio::sync::watch::Sender<Snapshot>>,
}
impl Supervisor {
    pub fn new(config: ValidatedConfig) -> Result<Self> {
        config.require_sources()?;
        Ok(Self {
            config,
            observer: None,
        })
    }
    pub fn with_observer(mut self, observer: tokio::sync::watch::Sender<Snapshot>) -> Self {
        self.observer = Some(observer);
        self
    }
    pub async fn run(self, cancel: CancellationToken) -> Result<Snapshot> {
        let transport = tokio::select! { biased;
            _ = cancel.cancelled() => return Err(Error::Cancelled),
            transport = Loopback::bind() => transport?,
        };
        self.run_with_transport(transport, cancel).await
    }

    /// Injection point for deterministic transport failure/backpressure tests.
    pub async fn run_with_transport<T: Transport>(
        self,
        mut transport: T,
        cancel: CancellationToken,
    ) -> Result<Snapshot> {
        let started = Instant::now();
        let cfg = self.config.get();
        let worker_cancel = cancel.child_token();
        let _cancel_on_drop = worker_cancel.clone().drop_guard();
        let (tx, mut rx) = mpsc::channel(cfg.runtime.queue_capacity);
        let mut transfer_id = [0; 16];
        getrandom::fill(&mut transfer_id)
            .map_err(|e| Error::Task(format!("OS random source: {e}")))?;
        let mut scanner = dandelion_scanner::start(
            dandelion_platform_linux::LinuxSourceAccess,
            ScanOptions {
                roots: cfg.scanner.roots.clone(),
                chunk_bytes: cfg.scanner.chunk_bytes,
                exclude_hidden: cfg.scanner.exclude_hidden,
                excluded_names: cfg.scanner.excluded_names.clone(),
                max_depth: cfg.scanner.max_depth,
                max_files: cfg.scanner.max_files,
                max_read_bytes: cfg.scanner.max_read_bytes,
            },
            transfer_id,
            tx,
            worker_cancel.clone(),
        );
        let mut snapshot = Snapshot::default();
        tracing::info!(
            queue_capacity = cfg.runtime.queue_capacity,
            "pipeline_started"
        );
        let sent = self
            .consume(&mut rx, &mut transport, &cancel, &mut snapshot)
            .await;
        // Close the blocking producer's output BEFORE waiting for its completion.
        rx.close();
        drop(rx);
        worker_cancel.cancel();
        let joined = tokio::time::timeout(
            Duration::from_millis(cfg.runtime.shutdown_timeout_ms),
            &mut scanner,
        )
        .await;
        let scan = match joined {
            Ok(result) => result.map_err(|e| Error::Task(e.to_string()))?,
            Err(_) => {
                scanner.abort();
                return Err(Error::ShutdownTimeout);
            }
        };
        match sent {
            Ok(()) => {
                snapshot.skipped_entries = scan?.skipped_entries;
            }
            Err(Error::Cancelled) => snapshot.status = RunStatus::Cancelled,
            Err(Error::BudgetExhausted) => snapshot.status = RunStatus::BudgetExhausted,
            Err(error) => return Err(error),
        }
        snapshot.elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        tracing::info!(status = ?snapshot.status, chunks=snapshot.chunks_sent, verified_bytes=snapshot.verified_payload_bytes, "pipeline_stopped");
        Ok(snapshot)
    }

    async fn consume<T: Transport>(
        &self,
        rx: &mut mpsc::Receiver<Chunk>,
        transport: &mut T,
        cancel: &CancellationToken,
        snapshot: &mut Snapshot,
    ) -> Result<()> {
        let cfg = self.config.get();
        let mut budget = TrafficBudget::new(cfg.pacing.traffic_limit_bytes)?;
        let mut rate = TokenBucket::new(cfg.pacing.bytes_per_second, cfg.pacing.burst_bytes)?;
        let mut encoded = Vec::with_capacity(MAX_DATAGRAM_BYTES);
        loop {
            let chunk = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                chunk = rx.recv() => match chunk { Some(chunk) => chunk, None => break },
            };
            dandelion_packet::encode(&chunk, &mut encoded)?;
            let wire_cost = transport.wire_cost(&encoded);
            budget.reserve(wire_cost)?;
            rate.acquire_scheduled(encoded.len() as u64, cancel, cfg.pacing.schedule.as_ref())
                .await?;
            snapshot.attempted_wire_bytes += wire_cost;
            let receipt = tokio::select! { biased;
                _ = cancel.cancelled() => return Err(Error::Cancelled),
                result = transport.transmit(&encoded, cancel) => result?,
            };
            snapshot.accepted_wire_bytes += receipt.accepted_wire_bytes;
            snapshot.verified_payload_bytes += receipt.verified_payload_bytes;
            snapshot.chunks_sent += 1;
            if chunk.last {
                snapshot.files_completed += 1;
            }
            if let Some(observer) = &self.observer {
                observer.send_replace(snapshot.clone());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dandelion_config::Config;
    use dandelion_transport::Receipt;
    fn fixture() -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("payload"), vec![42; 8193]).unwrap();
        let mut cfg = Config::default();
        cfg.scanner.roots.push(dir.path().into());
        cfg.runtime.queue_capacity = 1;
        (dir, cfg)
    }
    #[tokio::test]
    async fn full_pipeline_verifies_exact_payload() {
        let (_dir, cfg) = fixture();
        let stats = Supervisor::new(cfg.validate().unwrap())
            .unwrap()
            .run(CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(stats.files_completed, 1);
        assert_eq!(stats.verified_payload_bytes, 8193);
        assert_eq!(stats.chunks_sent, 8);
        assert_eq!(stats.accepted_wire_bytes, 8193 + 8 * 80);
        assert_eq!(stats.status, RunStatus::Completed);
    }
    struct Stalled;
    impl Transport for Stalled {
        async fn transmit(&mut self, _: &[u8], _: &CancellationToken) -> Result<Receipt> {
            std::future::pending().await
        }
    }
    #[tokio::test]
    async fn cancellation_unblocks_even_a_stalled_transport() {
        let (_dir, cfg) = fixture();
        let cancel = CancellationToken::new();
        let child = cancel.clone();
        let worker = tokio::spawn(async move {
            Supervisor::new(cfg.validate().unwrap())
                .unwrap()
                .run_with_transport(Stalled, child)
                .await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let stats = tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(stats.status, RunStatus::Cancelled);
    }
    struct Failed;
    impl Transport for Failed {
        async fn transmit(&mut self, _: &[u8], _: &CancellationToken) -> Result<Receipt> {
            Err(Error::Protocol("injected failure"))
        }
    }
    #[tokio::test]
    async fn failure_closes_the_producer_queue() {
        let (_dir, cfg) = fixture();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            Supervisor::new(cfg.validate().unwrap())
                .unwrap()
                .run_with_transport(Failed, CancellationToken::new()),
        )
        .await
        .unwrap();
        assert!(matches!(result, Err(Error::Protocol("injected failure"))));
    }
    #[tokio::test]
    async fn budget_never_overshoots() {
        let (_dir, mut cfg) = fixture();
        cfg.pacing.traffic_limit_bytes = 1232;
        let stats = Supervisor::new(cfg.validate().unwrap())
            .unwrap()
            .run(CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(stats.accepted_wire_bytes, 1232);
        assert_eq!(stats.status, RunStatus::BudgetExhausted);
    }
    #[tokio::test]
    async fn nonexistent_root_is_not_success() {
        let (dir, mut cfg) = fixture();
        cfg.scanner.roots = vec![dir.path().join("absent")];
        assert!(
            Supervisor::new(cfg.validate().unwrap())
                .unwrap()
                .run(CancellationToken::new())
                .await
                .is_err()
        );
    }
}
