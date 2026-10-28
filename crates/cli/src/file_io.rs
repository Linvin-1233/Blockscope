use crate::CliError;
use std::fs::{self, File};
use std::io::{self, BufRead, Read};
use std::path::Path;

/// Default raw-input cap shared by parsing and supervisor routing hashes.
pub(crate) const DEFAULT_INPUT_LIMIT: usize = 64 * 1024 * 1024;

/// Maximum accepted size of one protocol line, including its line ending.
pub(crate) const MAX_REQUEST_LINE_BYTES: usize = 1024 * 1024;

pub(crate) enum LineRead {
    Line(String),
    TooLong,
    InvalidUtf8,
    Eof,
}

/// Read one line without ever buffering more than `max_bytes`.
///
/// Overlong lines are drained through their newline and reported as
/// [`LineRead::TooLong`], so the next call starts at a clean boundary.
pub(crate) fn read_line_limited<R: BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> io::Result<LineRead> {
    let mut bytes = Vec::new();

    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(if bytes.is_empty() {
                LineRead::Eof
            } else {
                finish_line(bytes)
            });
        }

        let (take, has_newline) = match buffer.iter().position(|&byte| byte == b'\n') {
            Some(index) => (index + 1, true),
            None => (buffer.len(), false),
        };

        if bytes.len().saturating_add(take) > max_bytes {
            reader.consume(take);
            if !has_newline {
                drain_line(reader)?;
            }
            return Ok(LineRead::TooLong);
        }

        bytes.extend_from_slice(&buffer[..take]);
        reader.consume(take);

        if has_newline {
            return Ok(finish_line(bytes));
        }
    }
}

fn finish_line(mut bytes: Vec<u8>) -> LineRead {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }

    match String::from_utf8(bytes) {
        Ok(line) => LineRead::Line(line),
        Err(_) => LineRead::InvalidUtf8,
    }
}

fn drain_line<R: BufRead>(reader: &mut R) -> io::Result<()> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(());
        }
        if let Some(index) = buffer.iter().position(|&byte| byte == b'\n') {
            reader.consume(index + 1);
            return Ok(());
        }
        let len = buffer.len();
        reader.consume(len);
    }
}

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

    #[test]
    fn reads_crlf_and_a_final_line_without_a_newline() {
        let input = b"one\r\ntwo";
        let mut reader = std::io::Cursor::new(input);

        assert!(matches!(
            read_line_limited(&mut reader, 16).unwrap(),
            LineRead::Line(line) if line == "one"
        ));
        assert!(matches!(
            read_line_limited(&mut reader, 16).unwrap(),
            LineRead::Line(line) if line == "two"
        ));
        assert!(matches!(
            read_line_limited(&mut reader, 16).unwrap(),
            LineRead::Eof
        ));
    }

    #[test]
    fn drains_overlong_and_invalid_lines_before_the_next_read() {
        let mut overlong = std::io::Cursor::new(b"abcdef\nok\n".to_vec());
        assert!(matches!(
            read_line_limited(&mut overlong, 4).unwrap(),
            LineRead::TooLong
        ));
        assert!(matches!(
            read_line_limited(&mut overlong, 4).unwrap(),
            LineRead::Line(line) if line == "ok"
        ));

        // Force the overlong line across several fill_buf chunks.
        let mut spanning =
            std::io::BufReader::with_capacity(4, std::io::Cursor::new(b"abcdefgh\nok\n".to_vec()));
        assert!(matches!(
            read_line_limited(&mut spanning, 4).unwrap(),
            LineRead::TooLong
        ));
        assert!(matches!(
            read_line_limited(&mut spanning, 4).unwrap(),
            LineRead::Line(line) if line == "ok"
        ));

        let mut invalid = std::io::Cursor::new(vec![0xff, b'\n', b'o', b'k', b'\n']);
        assert!(matches!(
            read_line_limited(&mut invalid, 16).unwrap(),
            LineRead::InvalidUtf8
        ));
        assert!(matches!(
            read_line_limited(&mut invalid, 16).unwrap(),
            LineRead::Line(line) if line == "ok"
        ));
    }
}
