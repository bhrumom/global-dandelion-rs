use clap::{Parser, Subcommand, ValueEnum};
use dandelion_config::Overrides;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "dandelion",
    version,
    about = "Bounded, verifiable data pipeline for Linux",
    long_about = "Global Dandelion: an unprivileged Rust CLI. Explicit source directories, bounded resources, safe cancellation, and verified local UDP. No arbitrary Internet transmission."
)]
pub struct Cli {
    #[arg(long, global = true, env = "DANDELION_CONFIG", value_name = "FILE")]
    pub config: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        help = "Emit machine-readable result JSON on stdout"
    )]
    pub json: bool,
    #[arg(long, global = true, default_value = "info", env = "DANDELION_LOG")]
    pub log_level: String,
    #[arg(long, global = true, value_enum, default_value = "text")]
    pub log_format: LogFormat,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum LogFormat {
    Text,
    Json,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read selected directories once and verify every datagram locally.
    Run(Run),
    /// Receive and verify TLS 1.3 data from authenticated clients (memory receipts).
    Receive(Receive),
    /// Read-only Linux inventory or bounded mount-change observations.
    Platform {
        #[command(subcommand)]
        command: PlatformCommand,
    },
    /// Inspect durable state or import the original traffic_state.json.
    State {
        #[command(subcommand)]
        command: StateCommand,
    },
    /// Read-only metadata inventory or paced surface reading (no sockets).
    Scan {
        #[command(subcommand)]
        command: ScanCommand,
    },
    /// Inspect the last persisted runtime snapshot, without changing state.
    Status {
        #[arg(long)]
        directory: PathBuf,
    },
    /// Print defaults or validate effective configuration without sending traffic.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Exercise the entire pipeline using a small disposable local fixture.
    SelfTest,
    /// Read-only platform and disk-space diagnostics.
    Doctor,
    /// Generate shell completions to stdout.
    Completions { shell: clap_complete::Shell },
}
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Init,
    Check,
}
#[derive(Debug, clap::Args)]
pub struct Run {
    #[arg(long, value_name="DIR", action=clap::ArgAction::Append)]
    pub source: Vec<PathBuf>,
    #[arg(long)]
    pub rate_bytes_per_second: Option<u64>,
    #[arg(long)]
    pub traffic_limit_bytes: Option<u64>,
    #[arg(long)]
    pub queue_capacity: Option<usize>,
    #[arg(long)]
    pub chunk_bytes: Option<usize>,
    #[arg(
        long,
        help = "Show validated configuration; do not scan files or open sockets"
    )]
    pub dry_run: bool,
    /// Run a finite number of passes, sharing one traffic budget.
    #[arg(long, default_value_t = 1)]
    pub repeat: u32,
    /// Continue until cancelled or the session traffic budget is exhausted.
    #[arg(long, conflicts_with = "repeat")]
    pub watch: bool,
    #[arg(long, default_value_t = 1000)]
    pub interval_ms: u64,
    /// Render a latest-value terminal dashboard on stderr.
    #[arg(long)]
    pub tui: bool,
}
impl Run {
    pub fn overrides(&self) -> Overrides {
        Overrides {
            roots: (!self.source.is_empty()).then(|| self.source.clone()),
            queue_capacity: self.queue_capacity,
            chunk_bytes: self.chunk_bytes,
            bytes_per_second: self.rate_bytes_per_second,
            traffic_limit_bytes: self.traffic_limit_bytes,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum PlatformCommand {
    Inspect,
    Mounts,
    Watch {
        #[arg(long, default_value_t = 10)]
        samples: u32,
        #[arg(long, default_value_t = 1000)]
        interval_ms: u64,
    },
}
#[derive(Debug, Subcommand)]
pub enum StateCommand {
    Inspect {
        #[arg(long)]
        directory: PathBuf,
    },
    Import {
        #[arg(long)]
        directory: PathBuf,
        #[arg(long)]
        file: PathBuf,
    },
}
#[derive(Debug, Subcommand)]
pub enum ScanCommand {
    Metadata {
        #[arg(long, required = true)]
        source: Vec<PathBuf>,
    },
    Read {
        #[arg(long, required = true)]
        source: Vec<PathBuf>,
        #[arg(long)]
        rate_bytes_per_second: Option<u64>,
    },
}

#[derive(Debug, clap::Args)]
pub struct Receive {
    #[arg(long, default_value = "127.0.0.1:20000")]
    pub bind: std::net::SocketAddr,
    #[arg(long)]
    pub allow_remote: bool,
    #[arg(long)]
    pub allowed_client: Vec<std::net::IpAddr>,
    #[arg(long)]
    pub ca_file: PathBuf,
    #[arg(long)]
    pub certificate_file: PathBuf,
    #[arg(long)]
    pub private_key_file: PathBuf,
    #[arg(long, default_value_t = 4)]
    pub max_sessions: usize,
    #[arg(long)]
    pub once: bool,
    #[arg(long, default_value_t = 67108864)]
    pub traffic_limit_bytes: u64,
    #[arg(long, default_value_t = 1048576)]
    pub rate_bytes_per_second: u64,
    #[arg(long, default_value_t = 10000)]
    pub timeout_ms: u64,
}
