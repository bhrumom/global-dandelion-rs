//! Narrow, unprivileged Linux capabilities. No commands, shell, clock mutation,
//! root requirement, or unsafe blocks. Secure source opens require Linux 5.6+.
use dandelion_core::filesystem::{FileStamp, SourceAccess, SourceFile, SourceRoot};
use dandelion_core::{Error, Result};
use rustix::fs::{Mode, OFlags, ResolveFlags};
use serde::Serialize;
use std::{fs::File, path::Path};
use std::{
    io::{self, Read},
    os::unix::fs::MetadataExt,
};

/// Unprivileged Linux implementation of the source-access port.
pub struct LinuxSourceAccess;
impl SourceAccess for LinuxSourceAccess {
    type Root = SourceDirectory;
    fn open_root(&self, path: &Path) -> Result<Self::Root> {
        SourceDirectory::open(path)
    }
}

pub struct LinuxSourceFile {
    file: File,
}
impl Read for LinuxSourceFile {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.file.read(buffer)
    }
}
impl SourceFile for LinuxSourceFile {
    fn stamp(&self) -> Result<FileStamp> {
        let m = self.file.metadata()?;
        Ok(FileStamp {
            len: m.len(),
            change_token: [
                i128::from(m.dev()),
                i128::from(m.ino()),
                i128::from(m.mtime()),
                i128::from(m.mtime_nsec()),
                i128::from(m.ctime()),
                i128::from(m.ctime_nsec()),
            ],
        })
    }
}
impl SourceRoot for SourceDirectory {
    type File = LinuxSourceFile;
    fn open_file(&self, relative: &Path) -> Result<Self::File> {
        Ok(LinuxSourceFile {
            file: self.open_regular(relative)?,
        })
    }
}

pub struct SourceDirectory {
    directory: File,
}
impl SourceDirectory {
    pub fn open(path: &Path) -> Result<Self> {
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        Ok(Self {
            directory: fd.into(),
        })
    }
    /// The kernel resolves every component beneath the already-open root.
    /// Directory rename and symlink substitution cannot escape that root.
    pub fn open_regular(&self, relative: &Path) -> Result<File> {
        let fd = rustix::fs::openat2(
            &self.directory,
            relative,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
            ResolveFlags::BENEATH
                | ResolveFlags::NO_SYMLINKS
                | ResolveFlags::NO_MAGICLINKS
                | ResolveFlags::NO_XDEV,
        )
        .map_err(std::io::Error::from)?;
        let file = File::from(fd);
        if !file.metadata()?.is_file() {
            return Err(Error::Scan("source is not a regular file".into()));
        }
        Ok(file)
    }
}

#[derive(Debug, Serialize)]
pub struct DiskSpace {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub reserve_bytes: u64,
}
impl DiskSpace {
    pub fn check(&self, additional_peak_bytes: u64) -> Result<()> {
        let required = self
            .reserve_bytes
            .checked_add(additional_peak_bytes)
            .ok_or_else(|| Error::DiskReserve("write estimate overflow".into()))?;
        if self.available_bytes < required {
            return Err(Error::DiskReserve(format!(
                "available={}, reserve={}, planned_peak={additional_peak_bytes}",
                self.available_bytes, self.reserve_bytes
            )));
        }
        Ok(())
    }
}

pub fn disk_space(path: &Path) -> Result<DiskSpace> {
    let s = rustix::fs::statvfs(path).map_err(std::io::Error::from)?;
    let total_bytes = s.f_blocks.saturating_mul(s.f_frsize);
    let available_bytes = s.f_bavail.saturating_mul(s.f_frsize);
    if total_bytes == 0 {
        return Err(Error::DiskReserve(
            "filesystem did not report capacity".into(),
        ));
    }
    Ok(DiskSpace {
        total_bytes,
        available_bytes,
        reserve_bytes: (1024 * 1024 * 1024).max(total_bytes.div_ceil(20)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn space_guard_preserves_reserve_and_rejects_overflow() {
        let disk = DiskSpace {
            total_bytes: 1000,
            available_bytes: 100,
            reserve_bytes: 80,
        };
        disk.check(20).unwrap();
        assert!(disk.check(21).is_err());
        assert!(disk.check(u64::MAX).is_err());
    }
    #[test]
    fn kernel_rejects_symlink_and_parent_escape() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ok"), b"ok").unwrap();
        std::os::unix::fs::symlink("ok", dir.path().join("link")).unwrap();
        let root = SourceDirectory::open(dir.path()).unwrap();
        assert!(root.open_regular(Path::new("ok")).is_ok());
        assert!(root.open_regular(Path::new("link")).is_err());
        assert!(root.open_regular(Path::new("../etc/passwd")).is_err());
        assert!(root.open_regular(Path::new("/etc/passwd")).is_err());
        assert!(root.open_regular(Path::new(".")).is_err());
    }
}
