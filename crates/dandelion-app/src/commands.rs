//! Command handlers compose real capabilities; unimplemented domains have no stubs.
use crate::{
    cli::{Cli, Command, ConfigCommand, PlatformCommand, ScanCommand, StateCommand},
    execution::{self, RunOptions, StorageGuard},
    output,
};
use clap::CommandFactory;
use dandelion_config::{Config, Overrides};
use dandelion_core::{Error, Result, RunStatus};
use std::{io::Read, time::Duration};

fn print_config(config: &Config, machine: bool) -> Result<()> {
    if machine {
        output::json(config)
    } else {
        output::text(&config.to_toml()?)
    }
}
fn load(cli: &Cli, overrides: Overrides) -> Result<dandelion_config::ValidatedConfig> {
    let mut cfg = Config::load(cli.config.as_deref(), std::env::vars_os(), overrides)?
        .get()
        .clone();
    if let Some(name) = cli
        .config
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
    {
        cfg.scanner.excluded_names.push(name.into());
    }
    cfg.validate()
}
pub fn execute(cli: &Cli) -> Result<u8> {
    match &cli.command {
        Command::Receive(args) => receive(args),
        Command::Update { command } => update(command),
        Command::Run(args) => {
            let cfg = load(cli, args.overrides())?;
            if args.dry_run {
                print_config(cfg.get(), cli.json)?;
                return Ok(0);
            }
            output::snapshot(
                execution::run(
                    cfg,
                    RunOptions {
                        repeat: args.repeat,
                        watch: args.watch,
                        interval_ms: args.interval_ms,
                        tui: args.tui,
                        surface: false,
                    },
                )?,
                cli.json,
            )
        }
        Command::Config {
            command: ConfigCommand::Init,
        } => {
            print_config(&Config::default(), cli.json)?;
            Ok(0)
        }
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            print_config(load(cli, Overrides::default())?.get(), cli.json)?;
            Ok(0)
        }
        Command::Doctor => {
            output::json(
                &serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"requires":"Linux 5.6+ openat2, no root","disk":dandelion_platform_linux::disk_space(&std::env::current_dir()?)?,"network":"no diagnostics traffic sent"}),
            )?;
            Ok(0)
        }
        Command::SelfTest => {
            let temp_root = std::env::temp_dir();
            dandelion_platform_linux::disk_space(&temp_root)?.check(1024 * 1024)?;
            let fixture = tempfile::tempdir_in(temp_root)?;
            std::fs::write(fixture.path().join("payload.bin"), vec![0x5a; 65537])?;
            std::fs::write(fixture.path().join("empty.bin"), [])?;
            let mut cfg = Config::default();
            cfg.scanner.roots.push(fixture.path().into());
            let stats = execution::run(cfg.validate()?, RunOptions::default())?;
            if stats.status == RunStatus::Completed
                && (stats.verified_payload_bytes != 65537 || stats.files_completed != 2)
            {
                return Err(Error::Protocol("self-test incomplete"));
            }
            output::snapshot(stats, cli.json)
        }
        Command::Completions { shell } => {
            let mut bytes = Vec::new();
            clap_complete::generate(*shell, &mut Cli::command(), "dandelion", &mut bytes);
            output::text(std::str::from_utf8(&bytes).map_err(|e| Error::Task(e.to_string()))?)?;
            Ok(0)
        }
        Command::Status { directory } => {
            output::json(&dandelion_state::inspect(directory)?)?;
            Ok(0)
        }
        Command::State { command } => {
            match command {
                StateCommand::Inspect { directory } => {
                    output::json(&dandelion_state::inspect(directory)?)?
                }
                StateCommand::Import { directory, file } => {
                    let mut bytes = Vec::new();
                    std::fs::File::open(file)?
                        .take(1024 * 1024 + 1)
                        .read_to_end(&mut bytes)?;
                    let mut store = dandelion_state::Store::open(directory, StorageGuard)?;
                    let imported = store.import_legacy(&bytes)?;
                    output::json(
                        &serde_json::json!({"imported":imported,"ledger":store.ledger()}),
                    )?;
                }
            }
            Ok(0)
        }
        Command::Platform { command } => {
            match command {
                PlatformCommand::Inspect => {
                    output::json(&dandelion_platform_linux::inventory::observe()?)?
                }
                PlatformCommand::Mounts => {
                    output::json(&dandelion_platform_linux::inventory::mounts()?)?
                }
                PlatformCommand::Watch {
                    samples,
                    interval_ms,
                } => {
                    if !(1..=10000).contains(samples) || !(100..=60000).contains(interval_ms) {
                        return Err(Error::Config(
                            "watch: samples 1..10000, interval 100..60000 ms".into(),
                        ));
                    }
                    execution::drive(Duration::from_secs(3), |cancel| async move {
                        let mut previous = Vec::new();
                        for sample in 0..*samples {
                            let table = tokio::task::spawn_blocking(
                                dandelion_platform_linux::inventory::mounts,
                            )
                            .await
                            .map_err(|e| Error::Task(e.to_string()))??;
                            output::json(
                                &serde_json::json!({"sample":sample,"changes":dandelion_platform_linux::inventory::diff_mounts(&previous,&table)}),
                            )?;
                            previous = table;
                            if sample + 1 < *samples {
                                tokio::select! {biased;_=cancel.cancelled()=>return Err(Error::Cancelled),_=tokio::time::sleep(Duration::from_millis(*interval_ms))=>{}}
                            }
                        }
                        Ok(())
                    })?;
                }
            }
            Ok(0)
        }
        Command::Scan { command } => match command {
            ScanCommand::Read {
                source,
                rate_bytes_per_second,
            } => {
                let cfg = load(
                    cli,
                    Overrides {
                        roots: Some(source.clone()),
                        bytes_per_second: *rate_bytes_per_second,
                        ..Default::default()
                    },
                )?;
                output::snapshot(
                    execution::run(
                        cfg,
                        RunOptions {
                            surface: true,
                            ..Default::default()
                        },
                    )?,
                    cli.json,
                )
            }
            ScanCommand::Metadata { source } => {
                let cfg = load(
                    cli,
                    Overrides {
                        roots: Some(source.clone()),
                        ..Default::default()
                    },
                )?;
                let c = cfg.get().scanner.clone();
                let options = dandelion_scanner::ScanOptions {
                    roots: c.roots,
                    chunk_bytes: c.chunk_bytes,
                    exclude_hidden: c.exclude_hidden,
                    excluded_names: c.excluded_names,
                    max_depth: c.max_depth,
                    max_files: c.max_files,
                    max_read_bytes: c.max_read_bytes,
                };
                let result = execution::drive(Duration::from_secs(3), |cancel| async move {
                    tokio::task::spawn_blocking(move || {
                        dandelion_scanner::inspect_metadata(options, cancel)
                    })
                    .await
                    .map_err(|e| Error::Task(e.to_string()))?
                })?;
                output::json(&result)?;
                Ok(0)
            }
        },
    }
}

fn receive(args: &crate::cli::Receive) -> Result<u8> {
    if !(1..=16).contains(&args.max_sessions)
        || !(100..=60000).contains(&args.timeout_ms)
        || args.traffic_limit_bytes == 0
        || !(1..=1_073_741_824).contains(&args.rate_bytes_per_second)
    {
        return Err(Error::Config(
            "receiver requires bounded sessions, timeout, rate and positive traffic budget".into(),
        ));
    }
    let mut checked = args.bind;
    if checked.port() == 0 {
        checked.set_port(1);
    }
    dandelion_core::targets::validate_unicast(checked)?;
    if !args.bind.ip().is_loopback() && (!args.allow_remote || args.allowed_client.is_empty()) {
        return Err(Error::Config(
            "non-loopback listen requires --allow-remote and an explicit --allowed-client list"
                .into(),
        ));
    }
    let ca = dandelion_platform_linux::read_credential(&args.ca_file, false)?;
    let cert = dandelion_platform_linux::read_credential(&args.certificate_file, false)?;
    let key = dandelion_platform_linux::read_credential(&args.private_key_file, true)?;
    let config = dandelion_transport::tls::server_config(&ca, &cert, &key)?;
    let report = execution::drive(Duration::from_secs(3), |cancel| async move {
        let meter = dandelion_transport::metered::WireBudget::new(
            args.traffic_limit_bytes,
            args.rate_bytes_per_second,
            1232,
        )?;
        let listener = tokio::net::TcpListener::bind(args.bind).await?;
        output::json(
            &serde_json::json!({"event":"ready","address":listener.local_addr()?,"mutual_tls":true}),
        )?;
        dandelion_transport::tls::serve(
            listener,
            config,
            meter,
            dandelion_transport::tls::ReceiveOptions {
                max_sessions: args.max_sessions,
                max_connections: args.once.then_some(1),
                timeout: Duration::from_millis(args.timeout_ms),
                allowed_clients: args.allowed_client.clone(),
            },
            cancel,
        )
        .await
    })?;
    let code = if report.cancelled {
        130
    } else if report.failed_sessions > 0 {
        4
    } else {
        0
    };
    output::json(&report)?;
    Ok(code)
}

fn update(command: &crate::cli::UpdateCommand) -> Result<u8> {
    use crate::cli::UpdateCommand;
    let trust = match command {
        UpdateCommand::Check(trust) | UpdateCommand::Stage { trust, .. } => trust,
    };
    let raw = match (&trust.manifest, &trust.manifest_url) {
        (Some(path), None) => dandelion_updater::read_small(path, 65536)?,
        (None, Some(url)) => dandelion_updater::fetch_small(url, 65536)?,
        _ => return Err(Error::Config("choose one manifest source".into())),
    };
    let signature = dandelion_updater::read_small(&trust.signature, 64)?;
    let key = dandelion_updater::read_small(&trust.public_key, 32)?;
    let target = format!("{}-unknown-linux-gnu", std::env::consts::ARCH);
    let verified = dandelion_updater::verify(
        &raw,
        &signature,
        &key,
        env!("CARGO_PKG_VERSION"),
        &target,
        dandelion_updater::now()?,
        trust.allow_rollback,
    )?;
    match command {
        UpdateCommand::Check(_) => output::json(verified.manifest())?,
        UpdateCommand::Stage {
            directory,
            artifact,
            allowed_host,
            ..
        } => {
            let staged = if let Some(path) = artifact {
                dandelion_updater::stage(
                    &verified,
                    std::fs::File::open(path)?,
                    directory,
                    &StorageGuard,
                )?
            } else {
                dandelion_updater::stage_download(
                    &verified,
                    directory,
                    allowed_host
                        .as_deref()
                        .ok_or_else(|| Error::Config("explicit download host required".into()))?,
                    &StorageGuard,
                )?
            };
            output::json(&staged)?;
        }
    }
    Ok(0)
}
