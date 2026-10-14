//! Filesystem capability ports. Implementations own platform-specific identity
//! and safe path resolution; the scanner depends on these contracts only.
use crate::Result;
use std::{io::Read, path::Path};

/// Handle-based snapshot. The opaque change token is chosen by the adapter and
/// must change when identity/content metadata changes; it is not a content hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub len: u64,
    pub change_token: [i128; 6],
}

pub trait SourceFile: Read + Send {
    fn stamp(&self) -> Result<FileStamp>;
}

/// A capability rooted in one explicitly configured directory. Implementations
/// must reject non-regular files and resolution outside the held root.
pub trait SourceRoot: Send {
    type File: SourceFile;
    fn open_file(&self, relative: &Path) -> Result<Self::File>;
}

/// Injected by the composition root, never discovered through mutable globals.
pub trait SourceAccess: Send + 'static {
    type Root: SourceRoot;
    fn open_root(&self, path: &Path) -> Result<Self::Root>;
}
