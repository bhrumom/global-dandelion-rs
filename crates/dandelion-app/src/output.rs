use dandelion_core::{Error, Result, RunStatus, Snapshot};
use std::io::{self, Write};
pub fn text(value: &str) -> Result<()> {
    match io::stdout().lock().write_all(value.as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(e) => Err(e.into()),
    }
}
pub fn json(value: &impl serde::Serialize) -> Result<()> {
    text(&(serde_json::to_string(value).map_err(|e| Error::Task(e.to_string()))? + "\n"))
}
pub fn snapshot(stats: Snapshot, machine: bool) -> Result<u8> {
    if machine {
        json(&stats)?;
    } else {
        text(&format!(
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
