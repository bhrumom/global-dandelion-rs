//! Synchronous bootstrap/durability boundaries around a supervised async service.
use crate::supervisor::Supervisor;
use dandelion_config::{Config, ValidatedConfig};
use dandelion_core::{Error, Result, RunStatus, Snapshot};
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub(crate) fn authorized_tls_peers(
    tls: &dandelion_config::Tls,
) -> Result<Vec<dandelion_core::targets::AuthorizedPeer>> {
    use dandelion_core::targets::{TargetPool, TargetSelector};

    if tls.target_pool.is_empty() {
        return dandelion_core::targets::authorize(&tls.peers, tls.wan_enabled, &tls.wan_allowlist);
    }

    let pool = TargetPool::new(&tls.target_pool)?;
    let mut selector = TargetSelector::new(&pool, tls.target_selection, tls.target_seed)?;
    let selected = selector.select(tls.target_count)?;
    tracing::info!(
        selection_mode = ?tls.target_selection,
        pool_entries = tls.target_pool.len(),
        pool_cardinality = pool.len(),
        selected_targets = ?selected.iter().map(|peer| peer.address).collect::<Vec<_>>(),
        "authorized_target_pool_selected"
    );
    let mut peers = tls.peers.clone();
    peers.extend(selected);
    dandelion_core::targets::authorize_with_pool(
        &peers,
        tls.wan_enabled,
        &tls.wan_allowlist,
        Some(&pool),
    )
}

pub struct StorageGuard;
impl dandelion_state::WriteGuard for StorageGuard {
    fn check(&self, directory: &Path, additional_peak_bytes: u64) -> Result<()> {
        dandelion_platform_linux::disk_space(directory)?.check(additional_peak_bytes)
    }
}

fn resume_field(hasher: &mut blake3::Hasher, bytes: &[u8]) -> Result<()> {
    let len = u64::try_from(bytes.len())
        .map_err(|_| Error::Config("resume identity field is too large".into()))?;
    hasher.update(&len.to_be_bytes());
    hasher.update(bytes);
    Ok(())
}

fn hash_resume_mount_binding(
    hasher: &mut blake3::Hasher,
    root: &Path,
    mount: &dandelion_platform_linux::inventory::Mount,
) -> Result<()> {
    resume_field(hasher, root.as_os_str().as_encoded_bytes())?;
    hasher.update(&mount.id.to_be_bytes());
    hasher.update(&mount.parent_id.to_be_bytes());
    resume_field(hasher, mount.device.as_bytes())?;
    resume_field(hasher, mount.root.as_os_str().as_encoded_bytes())?;
    resume_field(hasher, mount.mountpoint.as_os_str().as_encoded_bytes())?;
    resume_field(hasher, mount.filesystem.as_bytes())?;
    resume_field(hasher, mount.source.as_os_str().as_encoded_bytes())?;
    resume_field(hasher, mount.options.as_bytes())
}

fn extend_resume_mount_scope(
    hasher: &mut blake3::Hasher,
    roots: &[PathBuf],
    table: &[dandelion_platform_linux::inventory::Mount],
) -> Result<()> {
    let mut canonical = roots
        .iter()
        .map(std::fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    canonical.sort();
    canonical.dedup();
    if canonical.is_empty() {
        return Err(Error::Config(
            "resume requires at least one mounted source".into(),
        ));
    }
    for root in canonical {
        if root.parent().is_none() {
            return Err(Error::Config(
                "filesystem root is not a resumable data source".into(),
            ));
        }
        let mount = dandelion_platform_linux::inventory::source_mount_identity(table, &root)?;
        hash_resume_mount_binding(hasher, &root, &mount)?;
    }
    Ok(())
}
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub progress_json: bool,
    pub repeat: u32,
    pub watch: bool,
    pub interval_ms: u64,
    pub tui: bool,
    pub surface: bool,
    pub resume: bool,
}
impl Default for RunOptions {
    fn default() -> Self {
        Self {
            repeat: 1,
            progress_json: false,
            watch: false,
            interval_ms: 1000,
            tui: false,
            surface: false,
            resume: false,
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
    if options.resume
        && (!config.state.enabled || options.watch || options.repeat != 1 || options.surface)
    {
        return Err(Error::Config(
            "file resume requires enabled state, one pass and a transfer mode".into(),
        ));
    }
    let resume_mount_table = if options.resume {
        let table = dandelion_platform_linux::inventory::mounts()?;
        if !config.scanner.allowed_mount_prefixes.is_empty() {
            config.scanner.roots.extend(
                dandelion_platform_linux::inventory::authorized_mount_roots(
                    &table,
                    &config.scanner.allowed_mount_prefixes,
                )?,
            );
            config.scanner.allowed_mount_prefixes.clear();
        }
        Some(table)
    } else {
        None
    };
    let transport_policy =
        serde_json::to_vec(&config.transport).map_err(|e| Error::Config(e.to_string()))?;
    let mut resume_scope = blake3::Hasher::new();
    resume_scope.update(b"GDAN/file-resume/v2");
    resume_scope.update(&transport_policy);
    let tls_config = if matches!(config.transport.mode, dandelion_config::Mode::Tls)
        && !options.surface
    {
        let tls = &config.transport.tls;
        let ca = dandelion_platform_linux::read_credential(&tls.ca_file, false)?;
        let certificate = dandelion_platform_linux::read_credential(&tls.certificate_file, false)?;
        resume_scope.update(&ca);
        resume_scope.update(&certificate);
        let key = dandelion_platform_linux::read_credential(&tls.private_key_file, true)?;
        for path in [&tls.ca_file, &tls.certificate_file, &tls.private_key_file] {
            let name = path.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
                Error::Config(
                    "TLS credential filenames must be representable in the exclusion policy".into(),
                )
            })?;
            config.scanner.excluded_names.push(name.into());
        }
        Some(dandelion_transport::tls::client_config(
            &ca,
            &certificate,
            &key,
        )?)
    } else {
        None
    };
    if let Some(table) = &resume_mount_table {
        extend_resume_mount_scope(&mut resume_scope, &config.scanner.roots, table)?;
    }
    let mut state = if config.state.enabled && !options.surface {
        Some(dandelion_state::Store::open(
            &config.state.directory,
            StorageGuard,
        )?)
    } else {
        None
    };
    let checkpoints = if options.resume {
        config.scanner.max_files = config.scanner.max_files.min(4096);
        Some(std::sync::Arc::new(dandelion_state::FileCheckpoints::new(
            resume_scope.finalize().to_hex().to_string(),
            state
                .as_ref()
                .ok_or_else(|| Error::State("resume state missing".into()))?
                .ledger(),
        )?))
    } else {
        None
    };
    let runtime_checkpoints = checkpoints.clone();
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
        let mut progress_rx = rx.clone();
        let progress = options.progress_json.then(|| {
            let child = ui_cancel.clone();
            tokio::spawn(async move {
                let mut timer = tokio::time::interval(Duration::from_secs(1));
                for sample in 0..1800 {
                    tokio::select! {
                        biased;
                        _ = child.cancelled() => break,
                        _ = timer.tick() => {
                            let changed = progress_rx.has_changed().unwrap_or(false);
                            let snapshot = progress_rx.borrow_and_update().clone();
                            if changed && snapshot.chunks_sent > 0 {
                                crate::output::json(&serde_json::json!({
                                    "event": "progress",
                                    "sequence": sample + 1,
                                    "pass_verified_payload_bytes": snapshot.verified_payload_bytes,
                                    "pass_chunks_sent": snapshot.chunks_sent,
                                    "pass_files_completed": snapshot.files_completed,
                                }))?;
                            }
                        }
                    }
                }
                Ok::<(), Error>(())
            })
        });
        let ui = options.tui.then(|| {
            let child = ui_cancel.clone();
            tokio::task::spawn_blocking(move || dandelion_tui::observe(rx, child))
        });
        let output = service(config, options, cancel, tx, tls_config, runtime_checkpoints).await;
        ui_cancel.cancel();
        if let Some(progress) = progress {
            tokio::time::timeout(deadline, progress)
                .await
                .map_err(|_| Error::ShutdownTimeout)?
                .map_err(|error| Error::Task(error.to_string()))??;
        }
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
    if let (Some(store), Some(checkpoints)) = (&mut state, checkpoints) {
        let (scope, keys) = checkpoints.export()?;
        store.save_checkpoints(scope, keys)?;
    }
    result
}

async fn service(
    config: Config,
    options: RunOptions,
    cancel: CancellationToken,
    observer: tokio::sync::watch::Sender<Snapshot>,
    tls_config: Option<std::sync::Arc<rustls::ClientConfig>>,
    checkpoints: Option<std::sync::Arc<dandelion_state::FileCheckpoints>>,
) -> Result<Snapshot> {
    let started = Instant::now();
    let mut total = Snapshot::default();
    let mut cycles = 0u32;
    let mut last_mounts = Vec::new();
    let mut rate = dandelion_pacing::TokenBucket::new(
        config.pacing.bytes_per_second,
        config.pacing.burst_bytes,
    )?;
    let meter = dandelion_transport::metered::WireBudget::new(
        config.pacing.traffic_limit_bytes,
        config.pacing.bytes_per_second,
        config.pacing.burst_bytes,
    )?;
    meter.set_schedule(config.pacing.schedule.clone())?;
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
        let supervisor = Supervisor::new(cycle.validate()?)?
            .with_observer(observer.clone())
            .with_checkpoints(
                checkpoints
                    .clone()
                    .map(|c| c as std::sync::Arc<dyn dandelion_core::checkpoint::Checkpoints>),
            );
        let before = meter.totals()?;
        let mut stats = if options.surface {
            supervisor
                .run_with_transport_and_rate(
                    dandelion_transport::ReadVerifier,
                    cancel.clone(),
                    &mut rate,
                )
                .await?
        } else if let Some(tls_config) = &tls_config {
            let tls = &config.transport.tls;
            let peers = authorized_tls_peers(tls)?;
            let transport = dandelion_transport::tls::Peers::connect(
                &peers,
                tls_config.clone(),
                meter.clone(),
                Duration::from_millis(tls.timeout_ms),
                tls.connect_attempts,
                &cancel,
            )
            .await?;
            supervisor
                .run_with_transport_and_rate(transport, cancel.clone(), &mut rate)
                .await?
        } else {
            let transport = tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),transport=dandelion_transport::Loopback::bind()=>transport?};
            supervisor
                .run_with_transport_and_rate(transport, cancel.clone(), &mut rate)
                .await?
        };
        if tls_config.is_some() && !options.surface {
            let after = meter.totals()?;
            stats.attempted_wire_bytes = after.0 - before.0;
            stats.accepted_wire_bytes = after.1 - before.1;
        }
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
        for target in stats.targets {
            if let Some(existing) = total
                .targets
                .iter_mut()
                .find(|t| t.address == target.address)
            {
                existing.verified_packets = existing
                    .verified_packets
                    .checked_add(target.verified_packets)
                    .ok_or_else(|| Error::Task("target packet counter overflow".into()))?;
                existing.verified_payload_bytes = existing
                    .verified_payload_bytes
                    .checked_add(target.verified_payload_bytes)
                    .ok_or_else(|| Error::Task("target payload counter overflow".into()))?;
                existing.accepted_ciphertext_bytes = existing
                    .accepted_ciphertext_bytes
                    .checked_add(target.accepted_ciphertext_bytes)
                    .ok_or_else(|| Error::Task("target wire counter overflow".into()))?;
            } else {
                total.targets.push(target);
            }
        }
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

#[cfg(test)]
mod resume_scope_tests {
    use super::*;

    fn mount(id: u64, device: &str, source: &str) -> dandelion_platform_linux::inventory::Mount {
        dandelion_platform_linux::inventory::Mount {
            id,
            parent_id: 1,
            device: device.into(),
            root: "/".into(),
            mountpoint: "/media/source".into(),
            filesystem: "ext4".into(),
            source: source.into(),
            options: "rw,relatime".into(),
        }
    }

    fn digest(binding: &dandelion_platform_linux::inventory::Mount) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"GDAN/file-resume/v2");
        hash_resume_mount_binding(&mut hasher, Path::new("/media/source/data"), binding).unwrap();
        hasher.finalize().to_hex().to_string()
    }

    #[test]
    fn remount_or_replacement_invalidates_resume_scope() {
        let original = mount(41, "8:17", "/dev/sdb1");
        assert_eq!(digest(&original), digest(&original));
        assert_ne!(digest(&original), digest(&mount(42, "8:17", "/dev/sdb1")));
        assert_ne!(digest(&original), digest(&mount(41, "8:33", "/dev/sdc1")));
    }
}

#[cfg(test)]
mod target_pool_composition_tests {
    use super::*;

    #[test]
    fn shipping_composition_reauthorizes_selected_pool_targets() {
        let tls = dandelion_config::Tls {
            wan_enabled: true,
            target_pool: vec![dandelion_core::targets::TargetPoolEntry {
                cidr: "8.8.8.0/29".into(),
                port: 443,
                server_name: "example.invalid".into(),
            }],
            target_selection: dandelion_core::targets::TargetSelectionMode::DeterministicSeeded,
            target_count: 3,
            target_seed: Some(99),
            ..Default::default()
        };
        let peers = authorized_tls_peers(&tls).unwrap();
        assert_eq!(peers.len(), 3);
        let pool = dandelion_core::targets::TargetPool::new(&tls.target_pool).unwrap();
        for peer in peers {
            assert!(pool.contains(peer.address()));
        }
    }
}
