//! Synchronous bootstrap/durability boundaries around a supervised async service.
use crate::supervisor::Supervisor;
use dandelion_config::{Config, ValidatedConfig};
use dandelion_core::{Error, Result, RunStatus, Snapshot};
use std::{
    future::Future,
    path::Path,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub struct StorageGuard;
impl dandelion_state::WriteGuard for StorageGuard {
    fn check(&self, directory: &Path, additional_peak_bytes: u64) -> Result<()> {
        dandelion_platform_linux::disk_space(directory)?.check(additional_peak_bytes)
    }
}
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub repeat: u32,
    pub watch: bool,
    pub interval_ms: u64,
    pub tui: bool,
    pub surface: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            repeat: 1,
            watch: false,
            interval_ms: 1000,
            tui: false,
            surface: false,
        }
    }
}

pub fn drive<T, F, Fut>(deadline: Duration, make_work: F) -> Result<T>
where
    F: FnOnce(CancellationToken) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(4)
        .enable_all()
        .build()?;
    let result=runtime.block_on(async {
        use tokio::signal::unix::{SignalKind,signal};
        let mut interrupt=signal(SignalKind::interrupt())?;let mut terminate=signal(SignalKind::terminate())?;
        let cancel=CancellationToken::new();let work=make_work(cancel.clone());tokio::pin!(work);
        tokio::select!{
            result=&mut work=>result,
            _=interrupt.recv()=>{cancel.cancel();tokio::time::timeout(deadline,work).await.map_err(|_|Error::ShutdownTimeout)?},
            _=terminate.recv()=>{cancel.cancel();tokio::time::timeout(deadline,work).await.map_err(|_|Error::ShutdownTimeout)?},
        }
    });
    runtime.shutdown_timeout(Duration::ZERO);
    result
}

pub fn run(validated: ValidatedConfig, options: RunOptions) -> Result<Snapshot> {
    validated.require_sources()?;
    if options.repeat == 0
        || options.repeat > 100_000
        || !(100..=3_600_000).contains(&options.interval_ms)
        || (options.surface && options.watch)
    {
        return Err(Error::Config(
            "repeat must be 1..100000, interval 100..3600000 ms; surface mode is finite".into(),
        ));
    }
    if options.tui {
        dandelion_tui::check_terminal()?;
    }
    let mut config = validated.get().clone();
    let mut state = if config.state.enabled && !options.surface {
        Some(dandelion_state::Store::open(
            &config.state.directory,
            StorageGuard,
        )?)
    } else {
        None
    };
    let lease = match &mut state {
        Some(store) => Some(store.reserve(
            config.pacing.traffic_limit_bytes,
            config.state.lifetime_limit_bytes,
        )?),
        None => None,
    };
    if let Some(lease) = &lease {
        config.pacing.traffic_limit_bytes = lease.bytes();
    }
    if config.state.enabled
        && let Some(name) = config.state.directory.file_name().and_then(|s| s.to_str())
    {
        config.scanner.excluded_names.push(name.into());
    }
    let deadline = Duration::from_millis(config.runtime.shutdown_timeout_ms);
    let result = drive(deadline, |cancel| async move {
        let (tx, rx) = tokio::sync::watch::channel(Snapshot::default());
        let ui_cancel = cancel.child_token();
        let _ui_guard = ui_cancel.clone().drop_guard();
        let ui = options.tui.then(|| {
            let child = ui_cancel.clone();
            tokio::task::spawn_blocking(move || dandelion_tui::observe(rx, child))
        });
        let output = service(config, options, cancel, tx).await;
        ui_cancel.cancel();
        if let Some(ui) = ui {
            tokio::time::timeout(deadline, ui)
                .await
                .map_err(|_| Error::ShutdownTimeout)?
                .map_err(|e| Error::Task(e.to_string()))??;
        }
        output
    });
    // All network/source tasks are settled before refunding provably unused lease.
    // Any error preserves the whole reservation, preventing crash-based budget reset.
    if let (Some(store), Some(lease), Ok(snapshot)) = (&mut state, lease, &result) {
        store.settle(lease, snapshot.clone())?;
    }
    result
}

async fn service(
    config: Config,
    options: RunOptions,
    cancel: CancellationToken,
    observer: tokio::sync::watch::Sender<Snapshot>,
) -> Result<Snapshot> {
    let started = Instant::now();
    let mut total = Snapshot::default();
    let mut cycles = 0u32;
    let mut last_mounts = Vec::new();
    loop {
        if cancel.is_cancelled() {
            total.status = RunStatus::Cancelled;
            break;
        }
        let mut cycle = config.clone();
        if !cycle.scanner.allowed_mount_prefixes.is_empty() {
            let table = tokio::task::spawn_blocking(dandelion_platform_linux::inventory::mounts)
                .await
                .map_err(|e| Error::Task(e.to_string()))??;
            let changes = dandelion_platform_linux::inventory::diff_mounts(&last_mounts, &table);
            tracing::info!(
                added = changes.added.len(),
                removed = changes.removed.len(),
                "mount_inventory_changed"
            );
            cycle.scanner.roots.extend(
                dandelion_platform_linux::inventory::authorized_mount_roots(
                    &table,
                    &cycle.scanner.allowed_mount_prefixes,
                )?,
            );
            last_mounts = table;
        }
        if cycle.scanner.roots.is_empty() {
            return Err(Error::Config(
                "no mounted source matches the explicit source policy".into(),
            ));
        }
        let remaining = config
            .pacing
            .traffic_limit_bytes
            .saturating_sub(total.attempted_wire_bytes);
        if remaining == 0 {
            total.status = RunStatus::BudgetExhausted;
            break;
        }
        cycle.pacing.traffic_limit_bytes = remaining;
        let supervisor = Supervisor::new(cycle.validate()?)?.with_observer(observer.clone());
        let stats = if options.surface {
            supervisor
                .run_with_transport(dandelion_transport::ReadVerifier, cancel.clone())
                .await?
        } else {
            supervisor.run(cancel.clone()).await?
        };
        macro_rules! sum {
            ($field:ident) => {
                total.$field = total
                    .$field
                    .checked_add(stats.$field)
                    .ok_or_else(|| Error::Task(concat!(stringify!($field), " overflow").into()))?;
            };
        }
        sum!(files_completed);
        sum!(chunks_sent);
        sum!(attempted_wire_bytes);
        sum!(accepted_wire_bytes);
        sum!(verified_payload_bytes);
        sum!(skipped_entries);
        total.status = stats.status;
        cycles += 1;
        if total.status != RunStatus::Completed || (!options.watch && cycles >= options.repeat) {
            break;
        }
        tokio::select! {biased;
            _=cancel.cancelled()=>{total.status=RunStatus::Cancelled;break;},
            _=tokio::time::sleep(Duration::from_millis(options.interval_ms))=>{}
        }
    }
    total.elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    observer.send_replace(total.clone());
    Ok(total)
}
