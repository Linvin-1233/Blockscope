use crate::VideoError;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const MINIMUM_VERSION: (u32, u32) = (4, 4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFormat {
    Mp4,
    Gif,
}

impl VideoFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "mp4" => Some(Self::Mp4),
            "gif" => Some(Self::Gif),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(Self::parse)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
            Self::Gif => "gif",
        }
    }

    pub const fn supports_alpha(self) -> bool {
        matches!(self, Self::Gif)
    }

    pub const fn needs_even_dimensions(self) -> bool {
        matches!(self, Self::Mp4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Low,
    Medium,
    High,
}

impl Quality {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    pub const fn crf(self) -> &'static str {
        match self {
            Self::Low => "28",
            Self::Medium => "23",
            Self::High => "18",
        }
    }

    pub const fn preset(self) -> &'static str {
        match self {
            Self::Low => "veryfast",
            Self::Medium => "fast",
            Self::High => "medium",
        }
    }

    pub const fn gif_colors(self) -> u16 {
        match self {
            Self::Low => 128,
            Self::Medium => 200,
            Self::High => 256,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    Auto,
    Software,
    Nvenc,
    Qsv,
    Amf,
    VideoToolbox,
    Vaapi,
}

impl Encoder {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "software" => Some(Self::Software),
            "nvenc" => Some(Self::Nvenc),
            "qsv" => Some(Self::Qsv),
            "amf" => Some(Self::Amf),
            "videotoolbox" => Some(Self::VideoToolbox),
            "vaapi" => Some(Self::Vaapi),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Software => "software",
            Self::Nvenc => "nvenc",
            Self::Qsv => "qsv",
            Self::Amf => "amf",
            Self::VideoToolbox => "videotoolbox",
            Self::Vaapi => "vaapi",
        }
    }

    pub const fn is_software(self) -> bool {
        matches!(self, Self::Auto | Self::Software)
    }
}

/// A discovered ffmpeg executable and its parsed version.
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    path: PathBuf,
    version: (u32, u32),
}

impl Ffmpeg {
    pub fn discover(path: Option<&Path>) -> Result<Self, VideoError> {
        let path = path.map_or_else(|| PathBuf::from("ffmpeg"), Path::to_path_buf);
        let output = Command::new(&path)
            .arg("-version")
            .output()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => VideoError::ffmpeg_missing(format!(
                    "ffmpeg was not found at '{}'; install ffmpeg or pass --ffmpeg",
                    path.display()
                )),
                _ => VideoError::ffmpeg(format!(
                    "cannot run ffmpeg at '{}': {error}",
                    path.display()
                )),
            })?;
        if !output.status.success() {
            return Err(VideoError::ffmpeg(format!(
                "ffmpeg -version failed at '{}': {}",
                path.display(),
                stderr_tail(&output.stderr)
            )));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let Some(version) = parse_version(&text) else {
            return Err(VideoError::ffmpeg_version(format!(
                "cannot parse ffmpeg version from '{}'",
                text.lines().next().unwrap_or_default()
            )));
        };
        if version < MINIMUM_VERSION {
            return Err(VideoError::ffmpeg_version(format!(
                "ffmpeg {}.{} is too old; {} or newer is required",
                version.0, version.1, MINIMUM_VERSION.0
            )));
        }
        Ok(Self { path, version })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn version(&self) -> (u32, u32) {
        self.version
    }

    /// Resolve a requested encoder to the ffmpeg name. Hardware encoders are
    /// parsed but not enabled in this slice; explicit requests fail rather than
    /// silently falling back.
    pub fn resolve_encoder(
        &self,
        requested: Encoder,
        format: VideoFormat,
    ) -> Result<&'static str, VideoError> {
        if format == VideoFormat::Gif {
            return match requested {
                Encoder::Auto | Encoder::Software => Ok("gif"),
                other => Err(VideoError::encoder(format!(
                    "{} is not available for GIF output",
                    other.as_str()
                ))),
            };
        }
        match requested {
            Encoder::Auto | Encoder::Software => Ok("libx264"),
            other => Err(VideoError::encoder(format!(
                "hardware encoder '{}' is not enabled in this build; use --encoder software",
                other.as_str()
            ))),
        }
    }
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let line = text.lines().next()?;
    let rest = line.split_once("version ")?.1;
    for token in rest.split_whitespace() {
        let token = token.trim_start_matches(['n', 'N']);
        let mut parts = token.split('.');
        let Some(major) = parts.next().and_then(|part| part.parse::<u32>().ok()) else {
            continue;
        };
        let minor = parts
            .next()
            .and_then(|part| part.parse::<u32>().ok())
            .unwrap_or(0);
        return Some((major, minor));
    }
    None
}

pub(crate) fn stderr_tail(bytes: &[u8]) -> String {
    const LIMIT: usize = 64 * 1024;
    let start = bytes.len().saturating_sub(LIMIT);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{Encoder, Quality, VideoFormat, parse_version};

    #[test]
    fn parses_common_ffmpeg_version_lines() {
        assert_eq!(
            parse_version("ffmpeg version 6.1.1-full_build Copyright"),
            Some((6, 1))
        );
        assert_eq!(parse_version("ffmpeg version n7.0.2"), Some((7, 0)));
        assert_eq!(parse_version("ffmpeg version N-112233-gabcdef"), None);
        assert_eq!(parse_version("not ffmpeg"), None);
    }

    #[test]
    fn parses_formats_quality_and_encoders() {
        assert_eq!(VideoFormat::parse("MP4"), Some(VideoFormat::Mp4));
        assert_eq!(VideoFormat::parse("gif"), Some(VideoFormat::Gif));
        assert!(VideoFormat::Mp4.needs_even_dimensions());
        assert!(VideoFormat::Gif.supports_alpha());
        assert_eq!(Quality::parse("high"), Some(Quality::High));
        assert_eq!(Encoder::parse("NVENC"), Some(Encoder::Nvenc));
        assert!(Encoder::Auto.is_software());
    }

    #[test]
    fn hardware_encoders_fail_without_a_silent_fallback() {
        let ffmpeg = super::Ffmpeg {
            path: std::path::PathBuf::from("ffmpeg"),
            version: (6, 1),
        };
        assert_eq!(
            ffmpeg
                .resolve_encoder(Encoder::Software, VideoFormat::Mp4)
                .unwrap(),
            "libx264"
        );
        let error = ffmpeg
            .resolve_encoder(Encoder::Nvenc, VideoFormat::Mp4)
            .unwrap_err();
        assert_eq!(error.code, crate::VideoErrorCode::EncoderUnavailable);
        assert!(
            ffmpeg
                .resolve_encoder(Encoder::Nvenc, VideoFormat::Gif)
                .is_err()
        );
    }
}
