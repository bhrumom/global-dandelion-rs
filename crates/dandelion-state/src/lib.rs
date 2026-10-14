//! Single-writer versioned persistence. Reserve before side effects; settle only
//! after owned work ends. A crash conservatively charges the outstanding lease.
use dandelion_core::{Error, Result, Snapshot};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
const MAX_STATE: u64 = 1024 * 1024;
const MAX_IMPORTS: usize = 64;

pub use dandelion_core::storage::WriteGuard;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ledger {
    pub charged_bytes: u64,
    pub accepted_bytes: u64,
    pub verified_payload_bytes: u64,
    pub completed_runs: u64,
    pub last_snapshot: Option<Snapshot>,
    pub imported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checkpoints: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_scope: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u32,
    revision: u64,
    checksum: String,
    ledger: Ledger,
}
#[derive(Debug)]
pub struct Lease {
    bytes: u64,
    revision: u64,
}
impl Lease {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

/// State directory must be private and pre-existing parents must be trusted.
/// The stable lock inode is never renamed; the data file is atomically replaced.
pub struct Store<G: WriteGuard> {
    directory: PathBuf,
    _lock: File,
    guard: G,
    ledger: Ledger,
    revision: u64,
    pending: bool,
}

impl<G: WriteGuard> Store<G> {
    pub fn open(directory: &Path, guard: G) -> Result<Self> {
        let parent = directory
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !directory.exists() {
            guard.check(parent, 2 * MAX_STATE)?;
            fs::DirBuilder::new().mode(0o700).create(directory)?;
        }
        let metadata = fs::symlink_metadata(directory)?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(Error::State(
                "state directory must be a private real directory (mode 0700)".into(),
            ));
        }
        let directory = fs::canonicalize(directory)?;
        guard.check(&directory, 2 * MAX_STATE)?;
        let lock_path = directory.join("writer.lock");
        if let Ok(m) = fs::symlink_metadata(&lock_path)
            && (!m.is_file() || m.file_type().is_symlink() || m.nlink() != 1)
        {
            return Err(Error::State("unsafe state lock".into()));
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&lock_path)?;
        lock.try_lock()
            .map_err(|e| Error::State(format!("another process owns the state directory: {e}")))?;
        let (revision, ledger) =
            load(&directory.join("state.json"))?.unwrap_or((0, Ledger::default()));
        Ok(Self {
            directory,
            _lock: lock,
            guard,
            ledger,
            revision,
            pending: false,
        })
    }
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// A whole-run durable lease avoids fsync on every chunk. Unknown progress
    /// after crash remains charged. Settling explicitly returns provably unused bytes.
    pub fn reserve(&mut self, run_limit: u64, lifetime_limit: u64) -> Result<Lease> {
        if self.pending {
            return Err(Error::State("one outstanding lease per store".into()));
        }
        let bytes = run_limit.min(lifetime_limit.saturating_sub(self.ledger.charged_bytes));
        if bytes == 0 {
            return Err(Error::BudgetExhausted);
        }
        let mut next = self.ledger.clone();
        next.charged_bytes = next
            .charged_bytes
            .checked_add(bytes)
            .ok_or(Error::BudgetExhausted)?;
        self.commit(next)?;
        self.pending = true;
        Ok(Lease {
            bytes,
            revision: self.revision,
        })
    }
    pub fn settle(&mut self, lease: Lease, snapshot: Snapshot) -> Result<()> {
        if !self.pending
            || lease.revision != self.revision
            || snapshot.attempted_wire_bytes > lease.bytes
        {
            return Err(Error::State("invalid lease settlement".into()));
        }
        let mut next = self.ledger.clone();
        next.charged_bytes -= lease.bytes - snapshot.attempted_wire_bytes;
        next.accepted_bytes = next
            .accepted_bytes
            .checked_add(snapshot.accepted_wire_bytes)
            .ok_or_else(|| Error::State("accepted counter overflow".into()))?;
        next.verified_payload_bytes = next
            .verified_payload_bytes
            .checked_add(snapshot.verified_payload_bytes)
            .ok_or_else(|| Error::State("verified counter overflow".into()))?;
        if snapshot.status == dandelion_core::RunStatus::Completed {
            next.completed_runs = next
                .completed_runs
                .checked_add(1)
                .ok_or_else(|| Error::State("run counter overflow".into()))?;
        }
        next.last_snapshot = Some(snapshot);
        self.commit(next)?;
        self.pending = false;
        Ok(())
    }
    /// Import the exact Python save_state schema; no target/privilege enrollment.
    pub fn import_legacy(&mut self, bytes: &[u8]) -> Result<bool> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Legacy {
            current_month: String,
            total_sent_bytes: u64,
        }
        if self.pending || bytes.len() as u64 > MAX_STATE {
            return Err(Error::State(
                "cannot import during a lease or oversized input".into(),
            ));
        }
        let legacy: Legacy = serde_json::from_slice(bytes)
            .map_err(|e| Error::State(format!("legacy schema: {e}")))?;
        let month = legacy.current_month.as_bytes();
        if month.len() != 7
            || month[4] != b'-'
            || !month[..4].iter().all(u8::is_ascii_digit)
            || !month[5..].iter().all(u8::is_ascii_digit)
            || !(1..=12).contains(&legacy.current_month[5..].parse::<u8>().unwrap_or(0))
        {
            return Err(Error::State("legacy current_month must be YYYY-MM".into()));
        }
        let digest = blake3::hash(bytes).to_hex().to_string();
        if self.ledger.imported.contains(&digest) {
            return Ok(false);
        }
        if self.ledger.imported.len() == MAX_IMPORTS {
            return Err(Error::State("legacy import ledger is full".into()));
        }
        let mut next = self.ledger.clone();
        // Migration is a floor, not an additive double charge from repeated exports.
        next.charged_bytes = next.charged_bytes.max(legacy.total_sent_bytes);
        next.imported.push(digest);
        self.commit(next)?;
        Ok(true)
    }
    pub fn save_checkpoints(&mut self, scope: String, keys: Vec<String>) -> Result<()> {
        if keys.len() > 4096
            || keys
                .iter()
                .any(|k| k.len() != 64 || !k.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(Error::State("invalid checkpoint bounds".into()));
        }
        let mut next = self.ledger.clone();
        next.checkpoint_scope = Some(scope);
        next.checkpoints = keys;
        self.commit(next)
    }
    fn commit(&mut self, ledger: Ledger) -> Result<()> {
        let revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| Error::State("revision overflow".into()))?;
        let raw = serde_json::to_vec(&ledger).map_err(|e| Error::State(e.to_string()))?;
        let envelope = Envelope {
            schema_version: 1,
            revision,
            checksum: blake3::hash(&raw).to_hex().to_string(),
            ledger,
        };
        let bytes = serde_json::to_vec(&envelope).map_err(|e| Error::State(e.to_string()))?;
        if bytes.len() as u64 > MAX_STATE {
            return Err(Error::State("state exceeded 1 MiB".into()));
        }
        self.guard.check(&self.directory, 2 * MAX_STATE)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(self.directory.join("state.json"))
            .map_err(|e| Error::Io(e.error))?;
        File::open(&self.directory)?.sync_all()?;
        self.ledger = envelope.ledger;
        self.revision = revision;
        Ok(())
    }
}

fn load(path: &Path) -> Result<Option<(u64, Ledger)>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > MAX_STATE
    {
        return Err(Error::State("unsafe or oversized state file".into()));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_STATE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STATE {
        return Err(Error::State("state grew past size limit".into()));
    }
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|e| {
        Error::State(format!(
            "state corrupt; preserve it for explicit recovery: {e}"
        ))
    })?;
    if envelope.schema_version != 1 {
        return Err(Error::State(
            "unsupported state schema; refusing to reset traffic history".into(),
        ));
    }
    let raw = serde_json::to_vec(&envelope.ledger).map_err(|e| Error::State(e.to_string()))?;
    if envelope.checksum != blake3::hash(&raw).to_hex().as_str()
        || envelope.ledger.imported.len() > MAX_IMPORTS
    {
        return Err(Error::State(
            "state checksum/limits invalid; refusing reset".into(),
        ));
    }
    Ok(Some((envelope.revision, envelope.ledger)))
}

/// Read-only atomic snapshot; does not create directories or acquire a writer lock.
pub fn inspect(directory: &Path) -> Result<Option<Ledger>> {
    Ok(load(&directory.join("state.json"))?.map(|(_, ledger)| ledger))
}

/// Bounded acknowledged-file window, scoped to exact transport/credential policy.
/// Pending ids live only in this owned handle, never in process-global maps.
#[derive(Debug)]
pub struct FileCheckpoints {
    scope: String,
    state: std::sync::Mutex<CheckpointState>,
}
#[derive(Debug)]
struct CheckpointState {
    completed: std::collections::BTreeSet<String>,
    pending: std::collections::BTreeMap<u64, String>,
}
impl FileCheckpoints {
    pub fn new(scope: String, ledger: &Ledger) -> Result<Self> {
        let keys = if ledger.checkpoint_scope.as_deref() == Some(&scope) {
            ledger
                .checkpoints
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        } else {
            std::collections::BTreeSet::new()
        };
        if keys.len() > 4096 || keys.iter().any(|s: &String| s.len() != 64) {
            return Err(Error::State("invalid checkpoint history".into()));
        }
        Ok(Self {
            scope,
            state: std::sync::Mutex::new(CheckpointState {
                completed: keys,
                pending: std::collections::BTreeMap::new(),
            }),
        })
    }
    pub fn export(&self) -> Result<(String, Vec<String>)> {
        let state = self
            .state
            .lock()
            .map_err(|_| Error::State("checkpoint lock poisoned".into()))?;
        Ok((
            self.scope.clone(),
            state.completed.iter().cloned().collect(),
        ))
    }
}
impl dandelion_core::checkpoint::Checkpoints for FileCheckpoints {
    fn scope(&self) -> &str {
        &self.scope
    }
    fn contains(&self, key: &str) -> Result<bool> {
        Ok(self
            .state
            .lock()
            .map_err(|_| Error::State("checkpoint lock poisoned".into()))?
            .completed
            .contains(key))
    }
    fn register(&self, file_id: u64, key: String) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::State("checkpoint lock poisoned".into()))?;
        if state.pending.len() >= 4097 {
            return Err(Error::State("pending checkpoint limit exceeded".into()));
        }
        state.pending.insert(file_id, key);
        Ok(())
    }
    fn acknowledge(&self, file_id: u64) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::State("checkpoint lock poisoned".into()))?;
        let key = state
            .pending
            .remove(&file_id)
            .ok_or_else(|| Error::State("receipt missing source identity".into()))?;
        if state.completed.len() >= 4096 && !state.completed.contains(&key) {
            return Err(Error::State(
                "file-level checkpoint capacity is 4096; partition the source workload".into(),
            ));
        }
        state.completed.insert(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Guard;
    impl WriteGuard for Guard {
        fn check(&self, _: &Path, _: u64) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn persistent_lease_crash_and_settlement() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("state");
        let mut s = Store::open(&p, Guard).unwrap();
        let lease = s.reserve(100, 150).unwrap();
        let snap = Snapshot {
            attempted_wire_bytes: 40,
            accepted_wire_bytes: 40,
            verified_payload_bytes: 20,
            ..Default::default()
        };
        s.settle(lease, snap).unwrap();
        assert_eq!(s.ledger().charged_bytes, 40);
        let _crashed_lease = s.reserve(100, 150).unwrap();
        drop(s);
        let mut s = Store::open(&p, Guard).unwrap();
        assert_eq!(s.ledger().charged_bytes, 140);
        assert_eq!(s.reserve(100, 150).unwrap().bytes(), 10);
    }
    #[test]
    fn writer_lock_corruption_and_future_schema_fail_closed() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s");
        let mut s = Store::open(&p, Guard).unwrap();
        assert!(Store::open(&p, Guard).is_err());
        s.reserve(20, 40).unwrap();
        drop(s);
        let state = p.join("state.json");
        let original = fs::read(&state).unwrap();
        fs::write(&state, b"{").unwrap();
        assert!(Store::open(&p, Guard).is_err());
        assert_eq!(fs::read(&state).unwrap(), b"{");
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        value["schema_version"] = 2.into();
        fs::write(&state, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Store::open(&p, Guard).is_err());
    }
    #[test]
    fn import_is_bounded_idempotent_and_strict() {
        let d = tempfile::tempdir().unwrap();
        let mut s = Store::open(&d.path().join("s"), Guard).unwrap();
        let value = br#"{"current_month":"2026-09","total_sent_bytes":123}"#;
        assert!(s.import_legacy(value).unwrap());
        assert!(!s.import_legacy(value).unwrap());
        assert_eq!(s.ledger().charged_bytes, 123);
        assert!(
            s.import_legacy(br#"{"current_month":"2026-09","total_sent_bytes":-1}"#)
                .is_err()
        );
        assert!(
            s.import_legacy(
                br#"{"current_month":"2026-09","total_sent_bytes":1,"targets":["anything"]}"#
            )
            .is_err()
        );
    }
    #[test]
    fn guard_failure_prevents_state_creation() {
        struct Denied;
        impl WriteGuard for Denied {
            fn check(&self, _: &Path, _: u64) -> Result<()> {
                Err(Error::DiskReserve("test".into()))
            }
        }
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("no");
        assert!(Store::open(&p, Denied).is_err());
        assert!(!p.exists());
    }

    #[test]
    fn file_checkpoint_scope_receipts_and_restart() {
        use dandelion_core::checkpoint::Checkpoints;
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("s");
        let mut store = Store::open(&path, Guard).unwrap();
        let checkpoints = FileCheckpoints::new("policy-a".into(), store.ledger()).unwrap();
        let key = "a".repeat(64);
        checkpoints.register(1, key.clone()).unwrap();
        assert!(!checkpoints.contains(&key).unwrap());
        checkpoints.acknowledge(1).unwrap();
        let (scope, keys) = checkpoints.export().unwrap();
        store.save_checkpoints(scope, keys).unwrap();
        drop(store);
        let store = Store::open(&path, Guard).unwrap();
        assert!(
            FileCheckpoints::new("policy-a".into(), store.ledger())
                .unwrap()
                .contains(&key)
                .unwrap()
        );
        assert!(
            !FileCheckpoints::new("policy-b".into(), store.ledger())
                .unwrap()
                .contains(&key)
                .unwrap()
        );
        assert!(checkpoints.acknowledge(2).is_err());
    }
}
