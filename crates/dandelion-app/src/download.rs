//! Private filesystem adapter for the package-owned resource downloader.
use dandelion_core::{Error, Result};
use dandelion_download::{Report, Sink};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
struct Files {
    root: PathBuf,
    sequence: u64,
    progress: bool,
    last: Instant,
}
impl Sink for Files {
    fn reserve(&self, bytes: u64) -> Result<()> {
        dandelion_platform_linux::disk_space(&self.root)?.check(bytes)
    }
    fn create(&mut self, name: &str) -> Result<Box<dyn Write>> {
        if !name.starts_with("resource-") || name.contains(['/', '\\']) {
            return Err(Error::Config("invalid download name".into()));
        }
        self.reserve(65536)?;
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        Ok(Box::new(options.open(self.root.join(name))?))
    }
    fn progress(&mut self, bytes: u64, files: usize) -> Result<()> {
        if self.progress && self.last.elapsed() >= Duration::from_millis(100) {
            self.sequence += 1;
            self.last = Instant::now();
            crate::output::json(
                &serde_json::json!({"event":"progress","sequence":self.sequence,"pass_verified_payload_bytes":bytes,"pass_chunks_sent":files}),
            )?;
        }
        Ok(())
    }
}
pub fn run(directory: &Path, urls: &[String], progress: bool) -> Result<Report> {
    let metadata = fs::symlink_metadata(directory)?;
    if !directory.is_absolute()
        || !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || fs::read_dir(directory)?.next().is_some()
    {
        return Err(Error::Config(
            "empty private download directory required".into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Error::Config(
                "private download directory permissions required".into(),
            ));
        }
    }
    let mut files = Files {
        root: directory.to_path_buf(),
        sequence: 0,
        progress,
        last: Instant::now(),
    };
    files.reserve(1024 * 1024)?;
    dandelion_download::download(urls, &mut files)
}
