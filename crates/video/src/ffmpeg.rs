use crate::VideoError;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

pub const MINIMUM_VERSION: (u32, u32) = (4, 4);

/// A probe is one encoded frame; a stuck driver must not hang the worker.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const VAAPI_DEVICE: &str = "/dev/dri/renderD128";

type ProbeCache = Arc<Mutex<HashMap<(u32, u32), Vec<Encoder>>>>;

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

    /// VideoToolbox exposes a 0-100 quality scale where higher is better.
    pub const fn videotoolbox_quality(self) -> &'static str {
        match self {
            Self::Low => "50",
            Self::Medium => "65",
            Self::High => "80",
        }
    }

    /// AMF uses named quality presets, not a numeric quality value.
    pub const fn amf_quality(self) -> &'static str {
        match self {
            Self::Low => "speed",
            Self::Medium => "balanced",
            Self::High => "quality",
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

    pub const fn ffmpeg_name(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Software => Some("libx264"),
            Self::Nvenc => Some("h264_nvenc"),
            Self::Qsv => Some("h264_qsv"),
            Self::Amf => Some("h264_amf"),
            Self::VideoToolbox => Some("h264_videotoolbox"),
            Self::Vaapi => Some("h264_vaapi"),
        }
    }

    pub fn from_ffmpeg_name(name: &str) -> Option<Self> {
        match name {
            "libx264" => Some(Self::Software),
            "h264_nvenc" => Some(Self::Nvenc),
            "h264_qsv" => Some(Self::Qsv),
            "h264_amf" => Some(Self::Amf),
            "h264_videotoolbox" => Some(Self::VideoToolbox),
            "h264_vaapi" => Some(Self::Vaapi),
            _ => None,
        }
    }

    pub const fn is_software(self) -> bool {
        matches!(self, Self::Auto | Self::Software)
    }

    /// Probe order for `auto`. VideoToolbox is the native path on macOS.
    pub fn hardware_candidates() -> Vec<Self> {
        if cfg!(target_os = "macos") {
            vec![
                Self::VideoToolbox,
                Self::Nvenc,
                Self::Qsv,
                Self::Amf,
                Self::Vaapi,
            ]
        } else {
            vec![
                Self::Nvenc,
                Self::Qsv,
                Self::Amf,
                Self::VideoToolbox,
                Self::Vaapi,
            ]
        }
    }
}

/// An encoder name that ffmpeg accepts, plus the family that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedEncoder {
    encoder: Encoder,
    name: &'static str,
}

impl ResolvedEncoder {
    pub(crate) const fn new(encoder: Encoder, name: &'static str) -> Self {
        Self { encoder, name }
    }

    pub const fn encoder(self) -> Encoder {
        self.encoder
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn is_hardware(self) -> bool {
        !matches!(self.encoder, Encoder::Auto | Encoder::Software)
    }
}

/// A discovered ffmpeg executable, its version, and lazily cached probes.
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    path: PathBuf,
    version: (u32, u32),
    compiled: Arc<OnceLock<Vec<Encoder>>>,
    probes: ProbeCache,
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
        Ok(Self {
            path,
            version,
            compiled: Arc::new(OnceLock::new()),
            probes: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    #[cfg(test)]
    pub(crate) fn test_instance(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            version: MINIMUM_VERSION,
            compiled: Arc::new(OnceLock::new()),
            probes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub const fn version(&self) -> (u32, u32) {
        self.version
    }

    pub fn version_string(&self) -> String {
        format!("{}.{}", self.version.0, self.version.1)
    }

    /// H.264 encoders compiled into this ffmpeg build. An ffmpeg that cannot
    /// list encoders yields an empty list rather than failing discovery.
    pub fn compiled_encoders(&self) -> &[Encoder] {
        self.compiled.get_or_init(|| self.read_encoder_list())
    }

    fn read_encoder_list(&self) -> Vec<Encoder> {
        let Ok(output) = Command::new(&self.path)
            .args(["-hide_banner", "-encoders"])
            .output()
        else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        parse_encoder_list(&String::from_utf8_lossy(&output.stdout))
    }

    /// Names reported by `hello.video.encoders`: `auto`, `software`, and the
    /// hardware families linked into this ffmpeg. A listed encoder may still
    /// fail its device probe; `encoder` in the result is authoritative.
    pub fn video_encoders(&self) -> Vec<String> {
        let mut names = vec!["auto".to_string(), "software".to_string()];
        for encoder in Encoder::hardware_candidates() {
            if self.compiled_encoders().contains(&encoder) {
                names.push(encoder.as_str().to_string());
            }
        }
        names
    }

    /// Resolve a request to the ffmpeg encoder that will be used.
    ///
    /// `auto` probes hardware families in preference order at the requested
    /// output size, then falls back to libx264. An explicit hardware encoder
    /// must be compiled and usable at that size; it never falls back silently.
    pub fn resolve_encoder(
        &self,
        requested: Encoder,
        format: VideoFormat,
        width: u32,
        height: u32,
    ) -> Result<ResolvedEncoder, VideoError> {
        if format == VideoFormat::Gif {
            return match requested {
                Encoder::Auto | Encoder::Software => {
                    Ok(ResolvedEncoder::new(Encoder::Software, "gif"))
                }
                other => Err(VideoError::encoder(format!(
                    "{} is not available for GIF output",
                    other.as_str()
                ))),
            };
        }

        let software = ResolvedEncoder::new(Encoder::Software, "libx264");
        match requested {
            Encoder::Software => Ok(software),
            Encoder::Auto => Ok(self
                .usable_hardware(width, height)
                .first()
                .copied()
                .map(|encoder| {
                    ResolvedEncoder::new(
                        encoder,
                        encoder
                            .ffmpeg_name()
                            .expect("hardware encoders have an ffmpeg name"),
                    )
                })
                .unwrap_or(software)),
            other => {
                let name = other
                    .ffmpeg_name()
                    .expect("hardware encoders have an ffmpeg name");
                if !self.compiled_encoders().contains(&other) {
                    return Err(VideoError::encoder(format!(
                        "{} is not compiled into this ffmpeg build",
                        other.as_str()
                    )));
                }
                self.probe_encoder(other, width, height).map_err(|error| {
                    VideoError::encoder(format!(
                        "{} is not usable at {width}x{height}: {error}",
                        other.as_str()
                    ))
                })?;
                Ok(ResolvedEncoder::new(other, name))
            }
        }
    }

    fn usable_hardware(&self, width: u32, height: u32) -> Vec<Encoder> {
        let key = (width, height);
        if let Ok(cache) = self.probes.lock()
            && let Some(usable) = cache.get(&key)
        {
            return usable.clone();
        }

        let compiled = self.compiled_encoders();
        let mut usable = Vec::new();
        for encoder in Encoder::hardware_candidates() {
            if compiled.contains(&encoder) && self.probe_encoder(encoder, width, height).is_ok() {
                usable.push(encoder);
                break;
            }
        }
        if let Ok(mut cache) = self.probes.lock() {
            cache.insert(key, usable.clone());
        }
        usable
    }

    fn probe_encoder(&self, encoder: Encoder, width: u32, height: u32) -> Result<(), String> {
        let name = encoder
            .ffmpeg_name()
            .ok_or_else(|| "not a hardware encoder".to_string())?;
        let args = probe_args(name, encoder, width, height);
        let mut child = Command::new(&self.path)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot start probe: {error}"))?;

        let deadline = Instant::now() + PROBE_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    let output = child
                        .wait_with_output()
                        .map_err(|error| format!("cannot read probe result: {error}"))?;
                    if output.status.success() {
                        return Ok(());
                    }
                    let detail = stderr_tail(&output.stderr);
                    return Err(if detail.is_empty() {
                        format!("probe exited with {}", output.status)
                    } else {
                        detail
                    });
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "probe timed out after {} seconds",
                        PROBE_TIMEOUT.as_secs()
                    ));
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("cannot poll probe: {error}"));
                }
            }
        }
    }
}

fn probe_args(name: &str, encoder: Encoder, width: u32, height: u32) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    if encoder == Encoder::Vaapi {
        args.extend(["-vaapi_device".into(), VAAPI_DEVICE.into()]);
    }
    args.extend([
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "lavfi".into(),
        "-i".into(),
        format!("color=c=black:s={width}x{height}:d=0.04:r=25").into(),
    ]);
    if encoder == Encoder::Vaapi {
        args.extend(["-vf".into(), "format=nv12,hwupload".into()]);
    } else {
        args.extend(["-pix_fmt".into(), "yuv420p".into()]);
    }
    args.extend([
        "-frames:v".into(),
        "1".into(),
        "-an".into(),
        "-c:v".into(),
        name.into(),
        "-f".into(),
        "null".into(),
        "-".into(),
    ]);
    args
}

fn parse_encoder_list(text: &str) -> Vec<Encoder> {
    let mut found = Vec::new();
    for encoder in std::iter::once(Encoder::Software).chain(Encoder::hardware_candidates()) {
        let Some(name) = encoder.ffmpeg_name() else {
            continue;
        };
        let linked = text
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name));
        if linked {
            found.push(encoder);
        }
    }
    found
}

fn parse_version(text: &str) -> Option<(u32, u32)> {
    let line = text.lines().next()?;
    let rest = line.split_once("version ")?.1;
    for token in rest.split_whitespace() {
        if let Some(version) = parse_date_version(token) {
            return Some(version);
        }
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

/// Git/nightly builds print `YYYY-MM-DD-git-...` instead of `major.minor`.
fn parse_date_version(token: &str) -> Option<(u32, u32)> {
    let mut parts = token.split('-');
    let year = parts.next()?.parse::<u32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    if year < 2000 || !(1..=12).contains(&month) {
        return None;
    }
    Some((year, month))
}

pub(crate) fn stderr_tail(bytes: &[u8]) -> String {
    const LIMIT: usize = 64 * 1024;
    let start = bytes.len().saturating_sub(LIMIT);
    String::from_utf8_lossy(&bytes[start..]).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        Encoder, Quality, ResolvedEncoder, VideoFormat, parse_encoder_list, parse_version,
        probe_args,
    };

    #[test]
    fn parses_common_ffmpeg_version_lines() {
        assert_eq!(
            parse_version("ffmpeg version 6.1.1-full_build Copyright"),
            Some((6, 1))
        );
        assert_eq!(parse_version("ffmpeg version n7.0.2"), Some((7, 0)));
        assert_eq!(
            parse_version("ffmpeg version 2026-03-12-git-9dc44b43b2-essentials_build"),
            Some((2026, 3))
        );
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
        assert_eq!(Quality::parse("medium"), Some(Quality::Medium));
        assert_eq!(Quality::parse("low"), Some(Quality::Low));
        assert_eq!(Encoder::parse("NVENC"), Some(Encoder::Nvenc));
        assert_eq!(Encoder::parse("videotoolbox"), Some(Encoder::VideoToolbox));
        assert_eq!(Encoder::parse("vaapi"), Some(Encoder::Vaapi));
        assert!(Encoder::Auto.is_software());
        assert_eq!(Encoder::Nvenc.ffmpeg_name(), Some("h264_nvenc"));
        assert_eq!(
            Encoder::from_ffmpeg_name("h264_vaapi"),
            Some(Encoder::Vaapi)
        );
    }

    #[test]
    fn parses_ffmpeg_encoder_list_in_preference_order() {
        let text = "\
Encoders:
 V....D libx264              libx264 H.264 / AVC
 V....D h264_amf             AMD AMF H.264 Encoder
 V....D h264_nvenc           NVIDIA NVENC H.264 encoder
 V..... h264_qsv             H.264 / AVC
 V....D h264_vaapi           H.264/AVC (VAAPI)
";
        let encoders = parse_encoder_list(text);
        assert!(encoders.contains(&Encoder::Software));
        assert!(encoders.contains(&Encoder::Nvenc));
        assert!(encoders.contains(&Encoder::Qsv));
        assert!(encoders.contains(&Encoder::Amf));
        assert!(encoders.contains(&Encoder::Vaapi));
        assert!(!encoders.contains(&Encoder::VideoToolbox));
    }

    #[test]
    fn hardware_encoders_fail_without_a_silent_fallback() {
        let ffmpeg = super::Ffmpeg::test_instance(std::path::Path::new("ffmpeg"));
        assert_eq!(
            ffmpeg
                .resolve_encoder(Encoder::Software, VideoFormat::Mp4, 256, 256)
                .unwrap()
                .name(),
            "libx264"
        );
        assert_eq!(
            ffmpeg
                .resolve_encoder(Encoder::Auto, VideoFormat::Mp4, 256, 256)
                .unwrap()
                .name(),
            "libx264"
        );
        let error = ffmpeg
            .resolve_encoder(Encoder::Nvenc, VideoFormat::Mp4, 256, 256)
            .unwrap_err();
        assert_eq!(error.code, crate::VideoErrorCode::EncoderUnavailable);
        assert!(
            ffmpeg
                .resolve_encoder(Encoder::Nvenc, VideoFormat::Gif, 256, 256)
                .is_err()
        );
    }

    #[test]
    fn vaapi_probe_uploads_nv12_and_other_encoders_use_yuv420p() {
        let nvenc = probe_args("h264_nvenc", Encoder::Nvenc, 320, 240);
        let nvenc: Vec<_> = nvenc.iter().map(|arg| arg.to_string_lossy()).collect();
        assert!(nvenc.contains(&"-pix_fmt".into()));
        assert!(nvenc.contains(&"yuv420p".into()));
        assert!(!nvenc.contains(&"-vaapi_device".into()));

        let vaapi = probe_args("h264_vaapi", Encoder::Vaapi, 320, 240);
        let vaapi: Vec<_> = vaapi.iter().map(|arg| arg.to_string_lossy()).collect();
        assert!(vaapi.contains(&"-vaapi_device".into()));
        assert!(vaapi.contains(&"format=nv12,hwupload".into()));
        assert!(!vaapi.contains(&"yuv420p".into()));
    }

    #[test]
    fn resolved_encoder_reports_the_actual_family() {
        let software = ResolvedEncoder::new(Encoder::Software, "libx264");
        assert!(!software.is_hardware());
        let nvenc = ResolvedEncoder::new(Encoder::Nvenc, "h264_nvenc");
        assert!(nvenc.is_hardware());
        assert_eq!(nvenc.encoder(), Encoder::Nvenc);
    }
}
