/// Severity of a parser or renderer diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

impl DiagnosticSeverity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

/// Stable diagnostic codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    OutOfBoundsBlockEntity,
    DuplicateBlockEntity,
    MissingBlockEntityId,
    EntitiesIgnored,
    PendingTicksIgnored,
    RegionOverlap,
    MetadataMismatch,
    EmptyRegion,
    UnsupportedDataVersion,
}

impl DiagnosticCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutOfBoundsBlockEntity => "OUT_OF_BOUNDS_BLOCK_ENTITY",
            Self::DuplicateBlockEntity => "DUPLICATE_BLOCK_ENTITY",
            Self::MissingBlockEntityId => "MISSING_BLOCK_ENTITY_ID",
            Self::EntitiesIgnored => "ENTITIES_IGNORED",
            Self::PendingTicksIgnored => "PENDING_TICKS_IGNORED",
            Self::RegionOverlap => "REGION_OVERLAP",
            Self::MetadataMismatch => "METADATA_MISMATCH",
            Self::EmptyRegion => "EMPTY_REGION",
            Self::UnsupportedDataVersion => "UNSUPPORTED_DATA_VERSION",
        }
    }
}

/// A structured, non-fatal observation about a scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub region: Option<String>,
}

impl Diagnostic {
    pub fn new(
        code: DiagnosticCode,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            region: None,
        }
    }

    pub fn with_region(mut self, region: impl Into<String>) -> Self {
        self.region = Some(region.into());
        self
    }
}
