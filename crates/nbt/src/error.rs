use std::fmt;

/// Stable error codes for NBT decoding failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NbtErrorCode {
    InvalidTag,
    TruncatedInput,
    InvalidCompression,
    LimitExceeded,
    InvalidUtf8,
    TrailingBytes,
}

impl NbtErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTag => "INVALID_TAG",
            Self::TruncatedInput => "TRUNCATED_INPUT",
            Self::InvalidCompression => "INVALID_COMPRESSION",
            Self::LimitExceeded => "NBT_LIMIT_EXCEEDED",
            Self::InvalidUtf8 => "INVALID_UTF8",
            Self::TrailingBytes => "TRAILING_BYTES",
        }
    }
}

impl fmt::Display for NbtErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A decoding error with a stable code and the input byte offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NbtError {
    pub code: NbtErrorCode,
    pub offset: usize,
    pub message: String,
}

impl NbtError {
    pub fn new(code: NbtErrorCode, offset: usize, message: impl Into<String>) -> Self {
        Self {
            code,
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for NbtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} (offset {})",
            self.code, self.message, self.offset
        )
    }
}

impl std::error::Error for NbtError {}
