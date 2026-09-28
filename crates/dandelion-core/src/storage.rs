//! Injectable capacity policy for every persistent writer.
use crate::Result;
use std::path::Path;
pub trait WriteGuard: Send {
    fn check(&self, directory: &Path, additional_peak_bytes: u64) -> Result<()>;
}
