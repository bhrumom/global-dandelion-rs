use clap::Parser;
use dandelion_app::{
    cli::{Cli, LogFormat},
    commands,
};
use dandelion_core::{Error, Result};
use std::{
    io::{self, IsTerminal},
    process::ExitCode,
};
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
fn main() -> ExitCode {
    let cli = Cli::parse();
    match init_logging(&cli).and_then(|()| commands::execute(&cli)) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if cli.json {
                eprintln!(
                    "{}",
                    serde_json::json!({"error":error.to_string(),"exit_code":error.exit_code()})
                );
            } else {
                eprintln!("error: {error}");
            }
            ExitCode::from(error.exit_code())
        }
    }
}
