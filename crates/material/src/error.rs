use std::fmt;

/// Stable material and asset error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialErrorCode {
    AssetReadFailed,
    InvalidJson,
    AssetLimit,
    UnsupportedSource,
    ModelInvalid,
}

impl MaterialErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AssetReadFailed => "ASSET_READ_FAILED",
            Self::InvalidJson => "ASSET_INVALID_JSON",
            Self::AssetLimit => "ASSET_LIMIT",
            Self::UnsupportedSource => "ASSET_SOURCE_UNSUPPORTED",
            Self::ModelInvalid => "MODEL_INVALID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaterialError {
    pub code: MaterialErrorCode,
    pub message: String,
}

impl MaterialError {
    pub fn new(code: MaterialErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn read(message: impl Into<String>) -> Self {
        Self::new(MaterialErrorCode::AssetReadFailed, message)
    }

    pub fn json(message: impl Into<String>) -> Self {
        Self::new(MaterialErrorCode::InvalidJson, message)
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(MaterialErrorCode::AssetLimit, message)
    }

    pub fn source(message: impl Into<String>) -> Self {
        Self::new(MaterialErrorCode::UnsupportedSource, message)
    }

    pub fn model(message: impl Into<String>) -> Self {
        Self::new(MaterialErrorCode::ModelInvalid, message)
    }
}

impl fmt::Display for MaterialError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for MaterialError {}
