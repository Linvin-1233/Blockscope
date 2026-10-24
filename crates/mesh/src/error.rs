use std::fmt;

/// Stable mesh construction errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshErrorCode {
    MeshLimitExceeded,
    PageLimitExceeded,
    CoordinateOverflow,
}

impl MeshErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MeshLimitExceeded => "MESH_LIMIT_EXCEEDED",
            Self::PageLimitExceeded => "PAGE_LIMIT_EXCEEDED",
            Self::CoordinateOverflow => "COORDINATE_OVERFLOW",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshError {
    pub code: MeshErrorCode,
    pub message: String,
}

impl MeshError {
    pub fn new(code: MeshErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(MeshErrorCode::MeshLimitExceeded, message)
    }

    pub fn page_limit(message: impl Into<String>) -> Self {
        Self::new(MeshErrorCode::PageLimitExceeded, message)
    }

    pub fn coordinate(message: impl Into<String>) -> Self {
        Self::new(MeshErrorCode::CoordinateOverflow, message)
    }
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for MeshError {}
