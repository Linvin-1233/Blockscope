use crate::CliError;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

/// Default raw-input cap shared by parsing and supervisor routing hashes.
pub(crate) const DEFAULT_INPUT_LIMIT: usize = 64 * 1024 * 1024;

/// Read a regular file without exceeding `max_bytes`.
///
/// Metadata is checked first so oversized files and non-regular paths (FIFOs,
/// devices) are rejected before opening them. `take` still bounds the read in
/// case the file grows between the metadata check and the read.
pub(crate) fn read_file_limited(path: &Path, max_bytes: usize) -> Result<Vec<u8>, CliError> {
    let metadata = fs::metadata(path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    if !metadata.is_file() {
        return Err(CliError::input(format!(
            "INVALID_REQUEST: {} is not a regular file",
            path.display()
        )));
    }

    let limit = u64::try_from(max_bytes).unwrap_or(u64::MAX);
    if metadata.len() > limit {
        return Err(CliError::input(format!(
            "INPUT_TOO_LARGE: {} is {} bytes, limit is {max_bytes}",
            path.display(),
            metadata.len()
        )));
    }

    let file = File::open(path)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    let mut reader = file.take(limit.saturating_add(1));
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::input(format!("IO_ERROR: {}: {error}", path.display())))?;
    if bytes.len() > max_bytes {
        return Err(CliError::input(format!(
            "INPUT_TOO_LARGE: {} exceeds {max_bytes} bytes",
            path.display()
        )));
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_regular_files_up_to_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.bin");
        std::fs::write(&path, b"hello").unwrap();

        assert_eq!(read_file_limited(&path, 5).unwrap(), b"hello");
    }

    #[test]
    fn rejects_oversized_files_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("input.bin");
        std::fs::write(&path, b"hello!").unwrap();

        let error = read_file_limited(&path, 5).unwrap_err();
        assert!(error.message.starts_with("INPUT_TOO_LARGE:"), "{error}");
    }

    #[test]
    fn rejects_non_regular_files() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_file_limited(dir.path(), 1024).unwrap_err();
        assert!(error.message.starts_with("INVALID_REQUEST:"), "{error}");
    }

    #[test]
    fn default_limit_matches_the_parser() {
        assert_eq!(
            DEFAULT_INPUT_LIMIT,
            litematic_core::DecodeLimits::default().max_input_bytes
        );
    }
}
