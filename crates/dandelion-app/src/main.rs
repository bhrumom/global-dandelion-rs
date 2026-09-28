mod cli;
use clap::{CommandFactory, Parser};
use cli::{Cli, Command, ConfigCommand, LogFormat};
use dandelion_app::supervisor::Supervisor;
use dandelion_config::{Config, Overrides, ValidatedConfig};
use dandelion_core::{Error, Result, RunStatus, Snapshot};
use std::{
    io::{self, IsTerminal, Write},
    process::ExitCode,
    time::Duration,
};
use tokio_util::sync::CancellationToken;

fn write_stdout(text: &str) -> Result<()> {
    io::stdout().lock().write_all(text.as_bytes())?;
    Ok(())
}
fn json(value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string(value).map_err(|e| Error::Task(e.to_string()))?;
    write_stdout(&(text + "\n"))
}
fn print_config(config: &Config, machine: bool) -> Result<()> {
    if machine {
        json(config)
    } else {
        write_stdout(&config.to_toml()?)
    }
}

fn init_logging(cli: &Cli) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_new(&cli.log_level)
        .map_err(|e| Error::Config(e.to_string()))?;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .with_target(false);
    match cli.log_format {
        LogFormat::Json => builder.json().try_init(),
        LogFormat::Text => builder.with_ansi(io::stderr().is_terminal()).try_init(),
    }
    .map_err(|e| Error::Task(e.to_string()))
}

fn run(config: ValidatedConfig) -> Result<Snapshot> {
    let supervisor = Supervisor::new(config)?;
    // Fixed small executor: never scales to CPU_COUNT * 50.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(2)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut terminate = signal(SignalKind::terminate())?;
        let cancel = CancellationToken::new();
        let work = supervisor.run(cancel.clone());
        tokio::pin!(work);
        tokio::select! {
            result = &mut work => result,
            _ = interrupt.recv() => { cancel.cancel(); work.await },
            _ = terminate.recv() => { cancel.cancel(); work.await },
        }
    });
    // A kernel-blocked regular-file read is not interruptible in Rust. The
    // supervisor reports timeout; do not wait forever while dropping the runtime.
    runtime.shutdown_timeout(Duration::ZERO);
    result
}
fn output_snapshot(stats: Snapshot, machine: bool) -> Result<u8> {
    if machine {
        json(&stats)?;
    } else {
        write_stdout(&format!(
            "{:?}: {} files, {} chunks, {} wire bytes accepted, {} payload bytes verified, {} ms\n",
            stats.status,
            stats.files_completed,
            stats.chunks_sent,
            stats.accepted_wire_bytes,
            stats.verified_payload_bytes,
            stats.elapsed_ms
        ))?;
    }
    Ok(match stats.status {
        RunStatus::Completed => 0,
        RunStatus::Cancelled => 130,
        RunStatus::BudgetExhausted => 75,
    })
}
fn execute(cli: &Cli) -> Result<u8> {
    match &cli.command {
        Command::Run(args) => {
            // Config reads happen before starting the Tokio executor.
            let config =
                Config::load(cli.config.as_deref(), std::env::vars_os(), args.overrides())?;
            if args.dry_run {
                print_config(config.get(), cli.json)?;
                return Ok(0);
            }
            output_snapshot(run(config)?, cli.json)
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
            let config = Config::load(
                cli.config.as_deref(),
                std::env::vars_os(),
                Overrides::default(),
            )?;
            print_config(config.get(), cli.json)?;
            Ok(0)
        }
        Command::SelfTest => {
            let temp_root = std::env::temp_dir();
            dandelion_platform_linux::disk_space(&temp_root)?.check(1024 * 1024)?;
            let fixture = tempfile::tempdir_in(&temp_root)?;
            std::fs::write(fixture.path().join("payload.bin"), vec![0x5a; 65537])?;
            std::fs::write(fixture.path().join("empty.bin"), [])?;
            let mut config = Config::default();
            config.scanner.roots.push(fixture.path().into());
            let stats = run(config.validate()?)?;
            if stats.verified_payload_bytes != 65537
                || stats.files_completed != 2
                || stats.status != RunStatus::Completed
            {
                return Err(Error::Protocol(
                    "self-test did not verify the complete fixture",
                ));
            }
            output_snapshot(stats, cli.json)
        }
        Command::Doctor => {
            let disk = dandelion_platform_linux::disk_space(&std::env::current_dir()?)?;
            json(
                &serde_json::json!({"version":env!("CARGO_PKG_VERSION"), "os":std::env::consts::OS, "arch":std::env::consts::ARCH,
                "requires":"Linux 5.6+ openat2, no root", "disk":disk, "network":"no diagnostics traffic sent"}),
            )?;
            Ok(0)
        }
        Command::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "dandelion", &mut io::stdout());
            Ok(0)
        }
    }
}
fn main() -> ExitCode {
    let cli = Cli::parse();
    match init_logging(&cli).and_then(|()| execute(&cli)) {
        Ok(code) => ExitCode::from(code),
        Err(Error::Io(e)) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::json!({"error":error.to_string(), "exit_code":error.exit_code()})
                );
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}
