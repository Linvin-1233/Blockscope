use litematic_nbt::NbtError;
use std::fmt;

/// Stable error codes for litematic parsing failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LitematicErrorCode {
    InvalidNbt,
    InvalidLitematic,
    UnsupportedVersion,
    InvalidMetadata,
    InvalidRegionBounds,
    ResourceLimit,
    InvalidPalette,
    InvalidPaletteIndex,
    BlockStateDataLengthMismatch,
    UnsupportedPaletteFormat,
    InvalidBlockEntity,
    BlockEntityLimitExceeded,
}

impl LitematicErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidNbt => "INVALID_NBT",
            Self::InvalidLitematic => "INVALID_LITEMATIC",
            Self::UnsupportedVersion => "UNSUPPORTED_LITEMATIC_VERSION",
            Self::InvalidMetadata => "INVALID_METADATA",
            Self::InvalidRegionBounds => "INVALID_REGION_BOUNDS",
            Self::ResourceLimit => "RESOURCE_LIMIT",
            Self::InvalidPalette => "INVALID_PALETTE",
            Self::InvalidPaletteIndex => "INVALID_PALETTE_INDEX",
            Self::BlockStateDataLengthMismatch => "BLOCK_STATE_DATA_LENGTH_MISMATCH",
            Self::UnsupportedPaletteFormat => "UNSUPPORTED_PALETTE_FORMAT",
            Self::InvalidBlockEntity => "INVALID_BLOCK_ENTITY",
            Self::BlockEntityLimitExceeded => "BLOCK_ENTITY_LIMIT_EXCEEDED",
        }
    }
}

impl fmt::Display for LitematicErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A litematic parsing error with a stable code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitematicError {
    pub code: LitematicErrorCode,
    pub message: String,
}

impl LitematicError {
    pub fn new(code: LitematicErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for LitematicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for LitematicError {}

impl From<NbtError> for LitematicError {
    fn from(error: NbtError) -> Self {
        Self::new(LitematicErrorCode::InvalidNbt, error.to_string())
    }
}
