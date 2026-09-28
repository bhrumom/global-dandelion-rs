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
