use std::fmt;

/// Stable scene construction errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneErrorCode {
    EmptyScene,
    BoundsOverflow,
    RegionLayoutMismatch,
    StatisticsOverflow,
    InvalidLodFactor,
    LodAggregationFailed,
}

impl SceneErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyScene => "EMPTY_SCENE",
            Self::BoundsOverflow => "BOUNDS_OVERFLOW",
            Self::RegionLayoutMismatch => "REGION_LAYOUT_MISMATCH",
            Self::StatisticsOverflow => "STATISTICS_OVERFLOW",
            Self::InvalidLodFactor => "INVALID_LOD_FACTOR",
            Self::LodAggregationFailed => "LOD_AGGREGATION_FAILED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneError {
    pub code: SceneErrorCode,
    pub message: String,
}

impl SceneError {
    pub fn new(code: SceneErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for SceneError {}
