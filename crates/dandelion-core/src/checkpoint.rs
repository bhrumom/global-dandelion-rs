//! File-level resume capability. Receipt ownership lives outside the scanner.
use crate::Result;
pub trait Checkpoints: Send + Sync + std::fmt::Debug {
    fn scope(&self) -> &str;
    fn contains(&self, key: &str) -> Result<bool>;
    fn register(&self, file_id: u64, key: String) -> Result<()>;
    fn acknowledge(&self, file_id: u64) -> Result<()>;
}
