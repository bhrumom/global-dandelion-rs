//! Platform-independent transfer values and failure semantics.
pub mod filesystem;
pub mod process;
pub mod schedule;
pub mod storage;
pub mod targets;

use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};
use thiserror::Error;

pub const HEADER_BYTES: usize = 80;
pub const MAX_DATAGRAM_BYTES: usize = 1232;
pub const MAX_PAYLOAD_BYTES: usize = MAX_DATAGRAM_BYTES - HEADER_BYTES;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("I/O failure: {0}")]
    Io(#[from] io::Error),
    #[error("persistent state: {0}")]
    State(String),
    #[error("platform observation: {0}")]
    Platform(String),
    #[error("invalid packet: {0}")]
    Protocol(&'static str),
    #[error("operation cancelled")]
    Cancelled,
    #[error("traffic budget exhausted before next complete datagram")]
    BudgetExhausted,
    #[error("shutdown deadline exceeded (an OS filesystem call may still be blocked)")]
    ShutdownTimeout,
    #[error("local verifier timed out")]
    VerifierTimeout,
    #[error("source changed during reading: {0}")]
    SourceChanged(PathBuf),
    #[error("scanner: {0}")]
    Scan(String),
    #[error("task failed: {0}")]
    Task(String),
    #[error("insufficient disk reserve: {0}")]
    DiskReserve(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Config(_) => 2,
            Self::Protocol(_) | Self::VerifierTimeout => 4,
            Self::BudgetExhausted => 75,
            Self::Cancelled => 130,
            _ => 1,
        }
    }
}

/// File ids are unique within one transfer, not persisted across runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub transfer_id: [u8; 16],
    pub file_id: u64,
    pub index: u64,
    pub last: bool,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    #[default]
    Completed,
    Cancelled,
    BudgetExhausted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub status: RunStatus,
    pub files_completed: u64,
    pub chunks_sent: u64,
    pub attempted_wire_bytes: u64,
    pub accepted_wire_bytes: u64,
    pub verified_payload_bytes: u64,
    pub skipped_entries: u64,
    pub elapsed_ms: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            schema_version: 1,
            status: RunStatus::Completed,
            files_completed: 0,
            chunks_sent: 0,
            attempted_wire_bytes: 0,
            accepted_wire_bytes: 0,
            verified_payload_bytes: 0,
            skipped_entries: 0,
            elapsed_ms: 0,
        }
    }
}

/// Single-owner accounting; a reservation is never rolled back on ambiguous I/O.
#[derive(Debug)]
pub struct TrafficBudget {
    limit: u64,
    reserved: u64,
}

impl TrafficBudget {
    pub fn new(limit: u64) -> Result<Self> {
        if limit == 0 {
            return Err(Error::Config("traffic limit must be positive".into()));
        }
        Ok(Self { limit, reserved: 0 })
    }
    pub fn reserve(&mut self, bytes: u64) -> Result<()> {
        let next = self
            .reserved
            .checked_add(bytes)
            .ok_or(Error::BudgetExhausted)?;
        if next > self.limit {
            return Err(Error::BudgetExhausted);
        }
        self.reserved = next;
        Ok(())
    }
    pub fn reserved(&self) -> u64 {
        self.reserved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_budget_and_overflow_fail_closed() {
        let mut budget = TrafficBudget::new(100).unwrap();
        budget.reserve(100).unwrap();
        assert!(matches!(budget.reserve(1), Err(Error::BudgetExhausted)));
        assert_eq!(budget.reserved(), 100);
        let mut budget = TrafficBudget::new(u64::MAX).unwrap();
        budget.reserve(u64::MAX).unwrap();
        assert!(budget.reserve(1).is_err());
        assert!(TrafficBudget::new(0).is_err());
    }
}
