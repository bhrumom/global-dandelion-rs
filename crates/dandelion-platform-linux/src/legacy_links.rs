//! Opt-in compatibility with Python's hidden hard/symbolic link exercise.
//! Original filenames are never renamed. Crash recovery is confined to an owned
//! four-name journal, never a filename heuristic or directory-wide deletion.
use dandelion_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
const NAMES: [&str; 4] = ["hard-a", "hard-b", "soft-a", "soft-b"];
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema_version: u32,
    pub source: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub workspace: PathBuf,
    pub phase: String,
    pub completed_cycles: u32,
}
fn fail(message: &str) -> Error {
    Error::State(format!("legacy links: {message}"))
}
fn lock(directory: &Path) -> Result<File> {
    let path = directory.join("writer.lock");
    let fd = rustix::fs::open(
        &path,
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::CLOEXEC
            | rustix::fs::OFlags::NOFOLLOW,
        rustix::fs::Mode::from_raw_mode(0o600),
    )
    .map_err(std::io::Error::from)?;
    let file = File::from(fd);
    let m = file.metadata()?;
    if !m.is_file() || m.nlink() != 1 {
        return Err(fail("unsafe lock file"));
    }
    file.try_lock()
        .map_err(|_| fail("another operation owns this journal"))?;
    Ok(file)
}
fn save(journal: &Journal) -> Result<()> {
    super::disk_space(&journal.workspace)?.check(1024 * 1024)?;
    let mut temp = tempfile::NamedTempFile::new_in(&journal.workspace)?;
    serde_json::to_writer(&mut temp, journal)
        .map_err(|_| fail("journal path encoding unsupported"))?;
    temp.flush()?;
    temp.as_file().sync_all()?;
    temp.persist(journal.workspace.join("links.json"))
        .map_err(|e| Error::Io(e.error))?;
    File::open(&journal.workspace)?.sync_all()?;
    Ok(())
}
fn source_matches(j: &Journal) -> Result<()> {
    let m = fs::symlink_metadata(&j.source)
        .map_err(|_| fail("original missing; preserving the recovery link"))?;
    if !m.is_file() || m.file_type().is_symlink() || m.dev() != j.device || m.ino() != j.inode {
        return Err(fail("original identity changed; refusing cleanup"));
    }
    Ok(())
}
fn load(directory: &Path) -> Result<Journal> {
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(fail("workspace must be a private real directory"));
    }
    let workspace = fs::canonicalize(directory)?;
    if !workspace
        .file_name()
        .is_some_and(|s| s.as_bytes().starts_with(b".dandelion-link-"))
    {
        return Err(fail("directory is not an owned link workspace"));
    }
    let path = workspace.join("links.json");
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 65536 {
        return Err(fail("invalid journal file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(fail("oversized journal"));
    }
    let j: Journal =
        serde_json::from_slice(&bytes).map_err(|_| fail("malformed ownership journal"))?;
    if j.schema_version != 1
        || j.workspace != workspace
        || j.source.parent() != workspace.parent()
        || j.completed_cycles > 1000
        || !matches!(j.phase.as_str(), "active" | "cleaned")
    {
        return Err(fail("invalid journal scope/version"));
    }
    Ok(j)
}
fn remove_owned_links(j: &Journal) -> Result<()> {
    source_matches(j)?;
    for name in NAMES {
        let path = j.workspace.join(name);
        let m = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        let valid = if name.starts_with("hard") {
            m.is_file()
                && !m.file_type().is_symlink()
                && m.dev() == j.device
                && m.ino() == j.inode
                && m.nlink() >= 2
        } else {
            m.file_type().is_symlink() && fs::read_link(&path)? == j.source
        };
        if !valid {
            return Err(fail("unexpected object in owned slot; preserving it"));
        }
    }
    for name in NAMES {
        let path = j.workspace.join(name);
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    File::open(&j.workspace)?.sync_all()?;
    Ok(())
}
/// Returns the retained small journal directory for explicit audit/recovery.
/// The caller must not concurrently mutate the selected source or its parent.
pub fn exercise(
    source: &Path,
    cycles: u32,
    interval_ms: u64,
    cancel: CancellationToken,
) -> Result<Journal> {
    if !(1..=1000).contains(&cycles) || !(10..=1000).contains(&interval_ms) {
        return Err(Error::Config(
            "link exercise bounds: 1..1000 cycles, 10..1000 ms".into(),
        ));
    }
    let raw = fs::symlink_metadata(source)?;
    if !raw.is_file() || raw.file_type().is_symlink() {
        return Err(fail("source must be a real regular file"));
    }
    let source = fs::canonicalize(source)?;
    if source
        .components()
        .any(|c| c.as_os_str().as_bytes().starts_with(b"."))
        || source.extension().is_some_and(|e| {
            matches!(
                e.to_str(),
                Some("toml" | "json" | "pem" | "key" | "rs" | "py")
            )
        })
    {
        return Err(fail(
            "hidden, source-code, credential and configuration paths are excluded",
        ));
    }
    let parent = source
        .parent()
        .ok_or_else(|| fail("source needs a parent"))?;
    if parent.parent().is_none()
        || ["/etc", "/proc", "/sys", "/dev"]
            .iter()
            .any(|p| source.starts_with(p))
    {
        return Err(fail("system source paths are prohibited"));
    }
    let fd = super::SourceDirectory::open(parent)?.open_regular(
        source
            .file_name()
            .ok_or_else(|| fail("missing filename"))?
            .as_ref(),
    )?;
    let m = fd.metadata()?;
    super::disk_space(parent)?.check(1024 * 1024)?;
    let workspace = tempfile::Builder::new()
        .prefix(".dandelion-link-")
        .tempdir_in(parent)?
        .keep();
    let _lock = lock(&workspace)?;
    let mut journal = Journal {
        schema_version: 1,
        source,
        device: m.dev(),
        inode: m.ino(),
        workspace,
        phase: "active".into(),
        completed_cycles: 0,
    };
    save(&journal)?;
    fs::hard_link(&journal.source, journal.workspace.join("hard-a"))?;
    let created = fs::metadata(journal.workspace.join("hard-a"))?;
    if created.dev() != journal.device || created.ino() != journal.inode {
        return Err(fail(
            "source changed during link creation; preserve journal for inspection",
        ));
    }
    std::os::unix::fs::symlink(&journal.source, journal.workspace.join("soft-a"))?;
    File::open(&journal.workspace)?.sync_all()?;
    let directory = File::open(&journal.workspace)?;
    let result = (|| {
        for cycle in 0..cycles {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            source_matches(&journal)?;
            let (old, new) = if cycle % 2 == 0 {
                ("a", "b")
            } else {
                ("b", "a")
            };
            for kind in ["hard", "soft"] {
                rustix::fs::renameat_with(
                    &directory,
                    format!("{kind}-{old}"),
                    &directory,
                    format!("{kind}-{new}"),
                    rustix::fs::RenameFlags::NOREPLACE,
                )
                .map_err(std::io::Error::from)?;
            }
            journal.completed_cycles = cycle + 1;
            save(&journal)?;
            // Explicit compatibility cadence; never used for runtime transfer pacing.
            let until = std::time::Instant::now() + Duration::from_millis(interval_ms);
            while std::time::Instant::now() < until && !cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(())
    })();
    remove_owned_links(&journal)?;
    journal.phase = "cleaned".into();
    save(&journal)?;
    result?;
    Ok(journal)
}
pub fn recover(directory: &Path) -> Result<Journal> {
    let mut j = load(directory)?;
    let _lock = lock(&j.workspace)?;
    remove_owned_links(&j)?;
    j.phase = "cleaned".into();
    save(&j)?;
    Ok(j)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_owned_links_are_renamed_and_cleaned() {
        let d = tempfile::Builder::new()
            .prefix("dandelion-link-test-")
            .tempdir()
            .unwrap();
        let file = d.path().join("data.bin");
        fs::write(&file, b"payload").unwrap();
        let original = fs::metadata(&file).unwrap().ino();
        let result = exercise(&file, 2, 10, CancellationToken::new()).unwrap();
        assert_eq!(result.phase, "cleaned");
        assert_eq!(fs::read(&file).unwrap(), b"payload");
        assert_eq!(fs::metadata(&file).unwrap().ino(), original);
        assert!(NAMES.iter().all(|n| !result.workspace.join(n).exists()));
        assert_eq!(recover(&result.workspace).unwrap().phase, "cleaned");
    }
    #[test]
    fn missing_original_preserves_last_hardlink() {
        let d = tempfile::Builder::new()
            .prefix("dandelion-link-test-")
            .tempdir()
            .unwrap();
        let file = d.path().join("data.bin");
        fs::write(&file, b"payload").unwrap();
        let j = exercise(&file, 1, 10, CancellationToken::new()).unwrap();
        fs::hard_link(&file, j.workspace.join("hard-a")).unwrap();
        fs::remove_file(&file).unwrap();
        assert!(recover(&j.workspace).is_err());
        assert_eq!(fs::read(j.workspace.join("hard-a")).unwrap(), b"payload");
    }
}
