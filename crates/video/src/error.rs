use std::fmt;

/// Stable video error codes shared by the library and the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoErrorCode {
    AnimationInvalid,
    FfmpegNotFound,
    FfmpegVersionUnsupported,
    EncoderUnavailable,
    VideoTooLarge,
    VideoTempFailed,
    FfmpegFailed,
}

impl VideoErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AnimationInvalid => "ANIMATION_INVALID",
            Self::FfmpegNotFound => "FFMPEG_NOT_FOUND",
            Self::FfmpegVersionUnsupported => "FFMPEG_VERSION_UNSUPPORTED",
            Self::EncoderUnavailable => "ENCODER_UNAVAILABLE",
            Self::VideoTooLarge => "VIDEO_TOO_LARGE",
            Self::VideoTempFailed => "VIDEO_TEMP_FAILED",
            Self::FfmpegFailed => "FFMPEG_FAILED",
        }
    }

    /// Only ffmpeg and temporary-storage failures are worth retrying.
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::FfmpegFailed | Self::VideoTempFailed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoError {
    pub code: VideoErrorCode,
    pub message: String,
}

impl VideoError {
    pub fn new(code: VideoErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn animation(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::AnimationInvalid, message)
    }

    pub fn ffmpeg_missing(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::FfmpegNotFound, message)
    }

    pub fn ffmpeg_version(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::FfmpegVersionUnsupported, message)
    }

    pub fn encoder(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::EncoderUnavailable, message)
    }

    pub fn too_large(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::VideoTooLarge, message)
    }

    pub fn temp(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::VideoTempFailed, message)
    }

    pub fn ffmpeg(message: impl Into<String>) -> Self {
        Self::new(VideoErrorCode::FfmpegFailed, message)
    }
}

impl fmt::Display for VideoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for VideoError {}
