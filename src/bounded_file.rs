//! Shared bounded regular-file admission for authoring and executable tool packages.
use crate::Error;
pub use eal_tool_runtime::bounded_file::FileDigest;
use std::path::Path;

pub fn read(path: &Path, limit: usize, label: &str) -> Result<Vec<u8>, Error> {
    Ok(eal_tool_runtime::bounded_file::read(path, limit, label)?)
}
pub fn read_within(
    root: &Path,
    relative: &Path,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>, Error> {
    Ok(eal_tool_runtime::bounded_file::read_within(
        root, relative, limit, label,
    )?)
}
pub fn sha256(path: &Path, limit: usize, label: &str) -> Result<FileDigest, Error> {
    Ok(eal_tool_runtime::bounded_file::sha256(path, limit, label)?)
}
