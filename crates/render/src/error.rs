use std::fmt;

/// Stable renderer errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderErrorCode {
    InvalidOptions,
    OutputTooLarge,
    PngEncodeFailed,
    TempStorageFailed,
}

impl RenderErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidOptions => "INVALID_RENDER_OPTIONS",
            Self::OutputTooLarge => "OUTPUT_TOO_LARGE",
            Self::PngEncodeFailed => "PNG_ENCODE_FAILED",
            Self::TempStorageFailed => "TEMP_STORAGE_FAILED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError {
    pub code: RenderErrorCode,
    pub message: String,
}

impl RenderError {
    pub fn new(code: RenderErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(RenderErrorCode::InvalidOptions, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(RenderErrorCode::OutputTooLarge, message)
    }

    pub fn png(message: impl Into<String>) -> Self {
        Self::new(RenderErrorCode::PngEncodeFailed, message)
    }

    pub fn temp_storage(message: impl Into<String>) -> Self {
        Self::new(RenderErrorCode::TempStorageFailed, message)
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for RenderError {}
