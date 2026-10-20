//! One blocking producer, one bounded queue. Directory entries are streamed,
//! not collected/sorted into an unbounded list. Source files are never modified.
use dandelion_core::filesystem::{SourceAccess, SourceFile, SourceRoot};
use dandelion_core::{Chunk, Error, MAX_PAYLOAD_BYTES, Result};
use std::{io::Read, path::PathBuf};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub chunk_bytes: usize,
    pub exclude_hidden: bool,
    pub excluded_names: Vec<String>,
    pub max_depth: usize,
    pub max_files: u64,
    pub max_read_bytes: u64,
    pub checkpoints: Option<std::sync::Arc<dyn dandelion_core::checkpoint::Checkpoints>>,
}

#[derive(Clone, Debug, Default)]
pub struct ScanSummary {
    pub files_read: u64,
    pub payload_read: u64,
    pub skipped_entries: u64,
}

pub fn start<A: SourceAccess>(
    access: A,
    options: ScanOptions,
    transfer_id: [u8; 16],
    output: mpsc::Sender<Chunk>,
    cancel: CancellationToken,
) -> JoinHandle<Result<ScanSummary>> {
    tokio::task::spawn_blocking(move || scan(access, options, transfer_id, output, cancel))
}

fn scan<A: SourceAccess>(
    access: A,
    options: ScanOptions,
    transfer_id: [u8; 16],
    output: mpsc::Sender<Chunk>,
    cancel: CancellationToken,
) -> Result<ScanSummary> {
    if !(1..=MAX_PAYLOAD_BYTES).contains(&options.chunk_bytes)
        || options.roots.is_empty()
        || options.roots.len() > 64
        || !(1..=64).contains(&options.max_depth)
    {
        return Err(Error::Config(
            "invalid scanner limits or missing source roots".into(),
        ));
    }
    let mut roots = options
        .roots
        .iter()
        .map(std::fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    let mut unique: Vec<PathBuf> = Vec::with_capacity(roots.len());
    for root in roots {
        if root.parent().is_none() {
            return Err(Error::Config(
                "scanning the filesystem root is not permitted; select data directories".into(),
            ));
        }
        if !unique.iter().any(|parent| root.starts_with(parent)) {
            unique.push(root);
        }
    }
    let mut summary = ScanSummary::default();
    let mut files_considered = 0u64;
    for root in unique {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let directory = access.open_root(&root)?;
        // max_open > max_depth avoids WalkDir's collect-to-close fallback.
        let walker = WalkDir::new(&root)
            .follow_links(false)
            .same_file_system(true)
            .max_depth(options.max_depth)
            .max_open(options.max_depth + 1)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || ((!options.exclude_hidden
                        || !entry.file_name().as_encoded_bytes().starts_with(b"."))
                        && !options
                            .excluded_names
                            .iter()
                            .any(|name| entry.file_name() == name.as_str())
                        && !entry.file_type().is_symlink())
            });
        for entry in walker {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = entry.map_err(|e| Error::Scan(e.to_string()))?;
            if !entry.file_type().is_file() {
                summary.skipped_entries += 1;
                continue;
            }
            if files_considered >= options.max_files {
                return Err(Error::Scan("file count limit reached".into()));
            }
            files_considered += 1;
            let relative = entry
                .path()
                .strip_prefix(&root)
                .map_err(|e| Error::Scan(e.to_string()))?;
            let mut file = directory.open_file(relative)?;
            // Identity and metadata are sampled from the opened handle, not a
            // second path lookup that could resolve to a different object.
            let before = file.stamp()?;
            let mut identity = blake3::Hasher::new();
            if let Some(checkpoints) = &options.checkpoints {
                identity.update(checkpoints.scope().as_bytes());
            }
            identity.update(root.as_os_str().as_encoded_bytes());
            identity.update(&[0]);
            identity.update(relative.as_os_str().as_encoded_bytes());
            identity.update(&before.len.to_be_bytes());
            for item in before.change_token {
                identity.update(&item.to_be_bytes());
            }
            let source_key = identity.finalize().to_hex().to_string();
            if let Some(checkpoints) = &options.checkpoints
                && checkpoints.contains(&source_key)?
            {
                summary.skipped_entries += 1;
                continue;
            }

            if before.len > options.max_read_bytes.saturating_sub(summary.payload_read) {
                return Err(Error::Scan("source read budget would be exceeded".into()));
            }
            let file_id = summary
                .files_read
                .checked_add(1)
                .ok_or_else(|| Error::Scan("file id overflow".into()))?;
            if let Some(checkpoints) = &options.checkpoints {
                checkpoints.register(file_id, source_key)?;
            }
            let mut offset = 0;
            let mut index = 0;
            loop {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let remaining = before.len - offset;
                let length = remaining.min(options.chunk_bytes as u64) as usize;
                let mut payload = vec![0; length];
                if let Err(e) = file.read_exact(&mut payload) {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        return Err(Error::SourceChanged(entry.path().into()));
                    }
                    return Err(e.into());
                }
                offset += length as u64;
                let last = offset == before.len;
                if last && before != file.stamp()? {
                    return Err(Error::SourceChanged(entry.path().into()));
                }
                // The supervisor MUST drop/close the receiver before joining us.
                output
                    .blocking_send(Chunk {
                        transfer_id,
                        file_id,
                        index,
                        last,
                        payload,
                    })
                    .map_err(|_| Error::Cancelled)?;
                summary.payload_read += length as u64;
                if last {
                    break;
                }
                index += 1;
            }
            summary.files_read += 1;
        }
    }
    Ok(summary)
}

#[derive(Debug, serde::Serialize)]
pub struct MetadataSummary {
    pub regular_files: u64,
    pub directories: u64,
    pub total_file_bytes: u64,
    pub payload_bytes_read: u64,
}
/// Pure metadata traversal: it never opens file payloads and honors cancellation.
pub fn inspect_metadata(
    options: ScanOptions,
    cancel: CancellationToken,
) -> Result<MetadataSummary> {
    if options.roots.is_empty()
        || options.roots.len() > 64
        || !(1..=64).contains(&options.max_depth)
    {
        return Err(Error::Config("invalid metadata scan bounds".into()));
    }
    let mut roots = options
        .roots
        .iter()
        .map(std::fs::canonicalize)
        .collect::<std::io::Result<Vec<_>>>()?;
    roots.sort();
    roots.dedup();
    let mut unique = Vec::<PathBuf>::new();
    for root in roots {
        if root.parent().is_none() {
            return Err(Error::Config("filesystem root is not a data source".into()));
        }
        if !unique.iter().any(|p| root.starts_with(p)) {
            unique.push(root);
        }
    }
    let mut summary = MetadataSummary {
        regular_files: 0,
        directories: 0,
        total_file_bytes: 0,
        payload_bytes_read: 0,
    };
    for root in unique {
        let walker = WalkDir::new(root)
            .follow_links(false)
            .same_file_system(true)
            .max_depth(options.max_depth)
            .max_open(options.max_depth + 1)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || ((!options.exclude_hidden
                        || !entry.file_name().as_encoded_bytes().starts_with(b"."))
                        && !options
                            .excluded_names
                            .iter()
                            .any(|s| entry.file_name() == s.as_str()))
            });
        for entry in walker {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let entry = entry.map_err(|e| Error::Scan(e.to_string()))?;
            if entry.file_type().is_file() {
                if summary.regular_files >= options.max_files {
                    return Err(Error::Scan("metadata file count limit reached".into()));
                }
                summary.regular_files += 1;
                summary.total_file_bytes = summary
                    .total_file_bytes
                    .checked_add(
                        entry
                            .metadata()
                            .map_err(|e| Error::Scan(e.to_string()))?
                            .len(),
                    )
                    .ok_or_else(|| Error::Scan("metadata size overflow".into()))?;
            } else if entry.file_type().is_dir() {
                summary.directories += 1;
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dandelion_platform_linux::LinuxSourceAccess;
    fn options(root: PathBuf) -> ScanOptions {
        ScanOptions {
            roots: vec![root],
            chunk_bytes: 3,
            exclude_hidden: true,
            excluded_names: vec!["target".into()],
            max_depth: 16,
            max_files: 100,
            max_read_bytes: 1024 * 1024,
            checkpoints: None,
        }
    }
    #[tokio::test]
    async fn fixtures_empty_hidden_symlink_and_duplicate_roots() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data"), b"abcdefg").unwrap();
        std::fs::write(dir.path().join("empty"), b"").unwrap();
        std::fs::write(dir.path().join(".hidden"), b"not sent").unwrap();
        std::os::unix::fs::symlink("data", dir.path().join("link")).unwrap();
        let mut config = options(dir.path().into());
        config.roots.push(dir.path().into());
        let (tx, mut rx) = mpsc::channel(1);
        let worker = start(
            LinuxSourceAccess,
            config,
            [1; 16],
            tx,
            CancellationToken::new(),
        );
        let mut chunks = Vec::new();
        while let Some(c) = rx.recv().await {
            chunks.push(c);
        }
        let summary = worker.await.unwrap().unwrap();
        assert_eq!(summary.files_read, 2);
        assert_eq!(summary.payload_read, 7);
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks.iter().filter(|c| c.last).count(), 2);
        let bytes: Vec<u8> = chunks
            .iter()
            .flat_map(|c| c.payload.iter().copied())
            .collect();
        assert_eq!(bytes, b"abcdefg");
    }
    #[tokio::test]
    async fn stalled_queue_is_bounded_and_drop_unblocks_shutdown() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("data"), vec![1; 32768]).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let worker = start(
            LinuxSourceAccess,
            options(dir.path().into()),
            [1; 16],
            tx,
            cancel.clone(),
        );
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        assert!(rx.len() <= 1);
        assert!(!worker.is_finished());
        cancel.cancel();
        drop(rx);
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(Error::Cancelled)));
    }
    #[tokio::test]
    async fn non_utf8_name_supported() {
        use std::os::unix::ffi::OsStringExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir
            .path()
            .join(std::ffi::OsString::from_vec(vec![0xff, b'a']));
        let created = std::fs::write(&path, b"x");
        // APFS rejects invalid UTF-8 at file creation (EILSEQ), before the
        // scanner runs. Still exercise the real scanner with a Unicode name.
        #[cfg(target_vendor = "apple")]
        let created = match created {
            Err(error) if error.raw_os_error() == Some(92) => {
                std::fs::write(dir.path().join("菩提-🪷"), b"x")
            }
            result => result,
        };
        created.unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let worker = start(
            LinuxSourceAccess,
            options(dir.path().into()),
            [1; 16],
            tx,
            CancellationToken::new(),
        );
        assert_eq!(rx.recv().await.unwrap().payload, b"x");
        assert!(rx.recv().await.is_none());
        assert_eq!(worker.await.unwrap().unwrap().files_read, 1);
    }

    #[tokio::test]
    async fn source_shrink_never_emits_a_successful_terminal_chunk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data");
        std::fs::write(&path, vec![7; 16384]).unwrap();
        let (tx, mut rx) = mpsc::channel(1);
        let worker = start(
            LinuxSourceAccess,
            options(dir.path().into()),
            [1; 16],
            tx,
            CancellationToken::new(),
        );
        let first = rx.recv().await.unwrap();
        assert!(!first.last);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(0)
            .unwrap();
        while let Some(chunk) = rx.recv().await {
            assert!(!chunk.last);
        }
        assert!(matches!(
            worker.await.unwrap(),
            Err(Error::SourceChanged(_))
        ));
    }
}
