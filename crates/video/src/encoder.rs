use crate::VideoError;
use crate::ffmpeg::{
    Encoder, Ffmpeg, Quality, ResolvedEncoder, VAAPI_DEVICE, VideoFormat, stderr_tail,
};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};

const STDERR_LIMIT: usize = 1_048_576;
const GIF_TEMP_LIMIT: u64 = 4 * 1024 * 1024 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

/// Resolved video output settings. Sizes and transparent backgrounds are
/// validated before the scene is parsed.
#[derive(Debug, Clone)]
pub struct VideoSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub format: VideoFormat,
    pub quality: Quality,
    pub encoder: Encoder,
    pub looped: bool,
    pub transparent: bool,
    /// Mesh/rasterizer budget. FFmpeg gets a fraction of it.
    pub threads: u32,
}

impl VideoSettings {
    pub fn validate(&self) -> Result<(), VideoError> {
        if self.width == 0 || self.height == 0 {
            return Err(VideoError::animation(
                "video width and height must be positive",
            ));
        }
        if !(1..=60).contains(&self.fps) {
            return Err(VideoError::animation("fps must be between 1 and 60"));
        }
        if self.format.needs_even_dimensions()
            && (!self.width.is_multiple_of(2) || !self.height.is_multiple_of(2))
        {
            return Err(VideoError::animation(
                "MP4 output requires even width and height for yuv420p",
            ));
        }
        if self.format == VideoFormat::Mp4 && self.transparent {
            return Err(VideoError::animation(
                "MP4 cannot carry alpha; use GIF or an opaque background",
            ));
        }
        self.frame_bytes()?;
        Ok(())
    }

    pub fn frame_bytes(&self) -> Result<usize, VideoError> {
        (self.width as usize)
            .checked_mul(self.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| VideoError::animation("video dimensions overflow"))
    }

    fn encode_threads(&self) -> u32 {
        (self.threads / 4).clamp(1, 4)
    }
}

pub enum VideoEncoder {
    Mp4(Mp4Encoder),
    Gif(GifEncoder),
}

impl VideoEncoder {
    pub fn start(
        ffmpeg: &Ffmpeg,
        settings: &VideoSettings,
        output: &Path,
    ) -> Result<Self, VideoError> {
        let resolved = ffmpeg.resolve_encoder(
            settings.encoder,
            settings.format,
            settings.width,
            settings.height,
        )?;
        Self::start_resolved(ffmpeg, settings, resolved, output)
    }

    pub fn start_resolved(
        ffmpeg: &Ffmpeg,
        settings: &VideoSettings,
        resolved: ResolvedEncoder,
        output: &Path,
    ) -> Result<Self, VideoError> {
        settings.validate()?;
        match settings.format {
            VideoFormat::Mp4 => Ok(Self::Mp4(Mp4Encoder::start(
                ffmpeg, settings, resolved, output,
            )?)),
            VideoFormat::Gif => Ok(Self::Gif(GifEncoder::start(ffmpeg, settings, output)?)),
        }
    }

    pub fn encoder_name(&self) -> &'static str {
        match self {
            Self::Mp4(encoder) => encoder.resolved.name(),
            Self::Gif(_) => "gif",
        }
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<(), VideoError> {
        match self {
            Self::Mp4(encoder) => encoder.write_frame(rgba),
            Self::Gif(encoder) => encoder.write_frame(rgba),
        }
    }

    /// Close the encoder, atomically replace the destination, and return the
    /// final byte size.
    pub fn finish(self) -> Result<u64, VideoError> {
        match self {
            Self::Mp4(encoder) => encoder.finish(),
            Self::Gif(encoder) => encoder.finish(),
        }
    }
}

pub struct Mp4Encoder {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr_thread: Option<JoinHandle<Vec<u8>>>,
    temp: Option<PathBuf>,
    output: PathBuf,
    resolved: ResolvedEncoder,
    expected_frame_bytes: usize,
    frames: u64,
}

impl Mp4Encoder {
    fn start(
        ffmpeg: &Ffmpeg,
        settings: &VideoSettings,
        resolved: ResolvedEncoder,
        output: &Path,
    ) -> Result<Self, VideoError> {
        remove_stale_output_temps(output, "mp4");
        let temp = temporary_output_path(output, "mp4");
        let args = mp4_args(settings, resolved, &temp);
        let mut child = Command::new(ffmpeg.path())
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::NotFound => VideoError::ffmpeg_missing(format!(
                    "ffmpeg was not found at '{}'",
                    ffmpeg.path().display()
                )),
                _ => VideoError::ffmpeg(format!("cannot start ffmpeg: {error}")),
            })?;
        let stdin = child.stdin.take();
        let stderr_thread = child.stderr.take().map(drain_stderr);
        Ok(Self {
            child: Some(child),
            stdin,
            stderr_thread,
            temp: Some(temp),
            output: output.to_path_buf(),
            resolved,
            expected_frame_bytes: settings.frame_bytes()?,
            frames: 0,
        })
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<(), VideoError> {
        if rgba.len() != self.expected_frame_bytes {
            return Err(VideoError::ffmpeg(format!(
                "frame has {} bytes, expected {}",
                rgba.len(),
                self.expected_frame_bytes
            )));
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| VideoError::ffmpeg("ffmpeg input pipe is closed"))?;
        stdin.write_all(rgba).map_err(|error| {
            VideoError::ffmpeg(format!(
                "ffmpeg closed its input after {} frames: {error}",
                self.frames
            ))
        })?;
        self.frames += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64, VideoError> {
        if let Some(stdin) = self.stdin.take() {
            drop(stdin);
        }
        let Some(mut child) = self.child.take() else {
            return Err(VideoError::ffmpeg("ffmpeg process is already gone"));
        };
        let status = child
            .wait()
            .map_err(|error| VideoError::ffmpeg(format!("cannot wait for ffmpeg: {error}")))?;
        let stderr = self
            .stderr_thread
            .take()
            .and_then(|thread| thread.join().ok())
            .unwrap_or_default();
        if !status.success() {
            if let Some(temp) = self.temp.take() {
                let _ = fs::remove_file(temp);
            }
            return Err(VideoError::ffmpeg(format!(
                "ffmpeg exited with {status}{}",
                stderr_suffix(&stderr)
            )));
        }
        let Some(temp) = self.temp.take() else {
            return Err(VideoError::temp("ffmpeg temporary output is missing"));
        };
        replace_output(&temp, &self.output)?;
        file_size(&self.output)
    }
}

impl Drop for Mp4Encoder {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.stdin.take();
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
        if let Some(temp) = self.temp.take() {
            let _ = fs::remove_file(temp);
        }
    }
}

pub struct GifEncoder {
    settings: VideoSettings,
    ffmpeg: PathBuf,
    raw: Option<tempfile::NamedTempFile>,
    temp_output: PathBuf,
    output: PathBuf,
    bytes_written: u64,
}

impl GifEncoder {
    fn start(ffmpeg: &Ffmpeg, settings: &VideoSettings, output: &Path) -> Result<Self, VideoError> {
        remove_stale_output_temps(output, "gif");
        let parent = output.parent().unwrap_or_else(|| Path::new("."));
        let raw = tempfile::Builder::new()
            .prefix(".blockscope-gif-")
            .suffix(".rgba")
            .tempfile_in(parent)
            .map_err(|error| VideoError::temp(format!("cannot create GIF frame file: {error}")))?;
        Ok(Self {
            settings: settings.clone(),
            ffmpeg: ffmpeg.path().to_path_buf(),
            raw: Some(raw),
            temp_output: temporary_output_path(output, "gif"),
            output: output.to_path_buf(),
            bytes_written: 0,
        })
    }

    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<(), VideoError> {
        if rgba.len() != self.settings.frame_bytes()? {
            return Err(VideoError::ffmpeg("frame size does not match video size"));
        }
        let next = self.bytes_written.saturating_add(rgba.len() as u64);
        if next > GIF_TEMP_LIMIT {
            return Err(VideoError::too_large(format!(
                "GIF frame store would exceed {} GiB",
                GIF_TEMP_LIMIT / (1024 * 1024 * 1024)
            )));
        }
        let raw = self
            .raw
            .as_mut()
            .ok_or_else(|| VideoError::temp("GIF frame file is closed"))?;
        raw.as_file_mut()
            .write_all(rgba)
            .map_err(|error| VideoError::temp(format!("cannot write GIF frames: {error}")))?;
        self.bytes_written = next;
        Ok(())
    }

    pub fn finish(mut self) -> Result<u64, VideoError> {
        let Some(mut raw) = self.raw.take() else {
            return Err(VideoError::temp("GIF frame file is closed"));
        };
        raw.as_file_mut()
            .flush()
            .map_err(|error| VideoError::temp(format!("cannot flush GIF frames: {error}")))?;
        let raw_path = raw.into_temp_path();

        let parent = self.output.parent().unwrap_or_else(|| Path::new("."));
        let palette = tempfile::Builder::new()
            .prefix(".blockscope-palette-")
            .suffix(".png")
            .tempfile_in(parent)
            .map_err(|error| VideoError::temp(format!("cannot create GIF palette: {error}")))?;
        let palette_path = palette.into_temp_path();

        if let Err(error) = run_ffmpeg(
            &self.ffmpeg,
            gif_palette_args(&self.settings, &raw_path, &palette_path),
        ) {
            let _ = fs::remove_file(&self.temp_output);
            return Err(error);
        }
        if let Err(error) = run_ffmpeg(
            &self.ffmpeg,
            gif_encode_args(&self.settings, &raw_path, &palette_path, &self.temp_output),
        ) {
            let _ = fs::remove_file(&self.temp_output);
            return Err(error);
        }

        replace_output(&self.temp_output, &self.output)?;
        file_size(&self.output)
    }
}

impl Drop for GifEncoder {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.temp_output);
    }
}

fn mp4_args(settings: &VideoSettings, resolved: ResolvedEncoder, output: &Path) -> Vec<OsString> {
    let mut args = raw_input_args(settings, "pipe:0");
    if resolved.is_hardware() && resolved.encoder() == Encoder::Vaapi {
        args.splice(0..0, ["-vaapi_device".into(), VAAPI_DEVICE.into()]);
    }
    args.extend(["-an".into(), "-c:v".into(), resolved.name().into()]);
    match resolved.encoder() {
        Encoder::Software => args.extend([
            "-preset".into(),
            settings.quality.preset().into(),
            "-crf".into(),
            settings.quality.crf().into(),
        ]),
        Encoder::Nvenc => args.extend([
            "-preset".into(),
            "fast".into(),
            "-rc".into(),
            "vbr".into(),
            "-cq".into(),
            settings.quality.crf().into(),
            "-b:v".into(),
            "0".into(),
        ]),
        Encoder::Qsv => args.extend([
            "-preset".into(),
            "fast".into(),
            "-global_quality".into(),
            settings.quality.crf().into(),
        ]),
        Encoder::Amf => args.extend([
            "-quality".into(),
            settings.quality.amf_quality().into(),
            "-rc".into(),
            "cqp".into(),
            "-qp_i".into(),
            settings.quality.crf().into(),
            "-qp_p".into(),
            settings.quality.crf().into(),
            "-qp_b".into(),
            settings.quality.crf().into(),
        ]),
        Encoder::VideoToolbox => args.extend([
            "-q:v".into(),
            settings.quality.videotoolbox_quality().into(),
        ]),
        Encoder::Vaapi => {
            args.extend(["-vf".into(), "format=nv12,hwupload".into()]);
            args.extend(["-qp".into(), settings.quality.crf().into()]);
        }
        Encoder::Auto => unreachable!("auto is resolved before encoding"),
    }
    if resolved.encoder() == Encoder::Software {
        args.extend([
            "-threads".into(),
            settings.encode_threads().to_string().into(),
        ]);
    }
    if resolved.encoder() != Encoder::Vaapi {
        args.extend(["-pix_fmt".into(), "yuv420p".into()]);
    }
    args.extend([
        "-movflags".into(),
        "+faststart".into(),
        "-map_metadata".into(),
        "-1".into(),
        "-f".into(),
        "mp4".into(),
        output.as_os_str().to_os_string(),
    ]);
    args
}

fn gif_palette_args(settings: &VideoSettings, raw: &Path, palette: &Path) -> Vec<OsString> {
    let mut args = raw_input_args(settings, &raw.to_string_lossy());
    args.push("-vf".into());
    let mut filter = format!(
        "palettegen=max_colors={}:stats_mode=diff",
        settings.quality.gif_colors()
    );
    if settings.transparent {
        filter.push_str(":reserve_transparent=1");
    }
    args.push(filter.into());
    args.extend(["-y".into(), palette.as_os_str().to_os_string()]);
    args
}

fn gif_encode_args(
    settings: &VideoSettings,
    raw: &Path,
    palette: &Path,
    output: &Path,
) -> Vec<OsString> {
    let mut args = raw_input_args(settings, &raw.to_string_lossy());
    args.push("-i".into());
    args.push(palette.as_os_str().to_os_string());
    args.push("-lavfi".into());
    let mut filter = "paletteuse=dither=sierra2_4a".to_string();
    if settings.transparent {
        filter.push_str(":alpha_threshold=128");
    }
    args.push(filter.into());
    args.push("-loop".into());
    args.push(if settings.looped { "0" } else { "-1" }.into());
    args.extend(["-y".into(), output.as_os_str().to_os_string()]);
    args
}

fn raw_input_args(settings: &VideoSettings, input: &str) -> Vec<OsString> {
    vec![
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-f".into(),
        "rawvideo".into(),
        "-pixel_format".into(),
        "rgba".into(),
        "-video_size".into(),
        format!("{}x{}", settings.width, settings.height).into(),
        "-framerate".into(),
        settings.fps.to_string().into(),
        "-i".into(),
        input.into(),
    ]
}

fn run_ffmpeg(ffmpeg: &Path, args: Vec<OsString>) -> Result<(), VideoError> {
    let output = Command::new(ffmpeg)
        .args(&args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => VideoError::ffmpeg_missing(format!(
                "ffmpeg was not found at '{}'",
                ffmpeg.display()
            )),
            _ => VideoError::ffmpeg(format!("cannot start ffmpeg: {error}")),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(VideoError::ffmpeg(format!(
            "ffmpeg exited with {}{}",
            output.status,
            stderr_suffix(&output.stderr)
        )))
    }
}

fn drain_stderr(mut stderr: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut captured = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let remaining = STDERR_LIMIT.saturating_sub(captured.len());
                    captured.extend_from_slice(&chunk[..count.min(remaining)]);
                }
            }
        }
        captured
    })
}

fn stderr_suffix(bytes: &[u8]) -> String {
    let tail = stderr_tail(bytes);
    if tail.is_empty() {
        String::new()
    } else {
        format!(": {tail}")
    }
}

/// Remove sibling temp files left by a killed encoder for this output stem.
/// Callers must not run two jobs for the same output path concurrently.
fn remove_stale_output_temps(output: &Path, extension: &str) {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let prefix = format!(".{stem}.blockscope-");
    let suffix = format!(".{extension}");
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with(&prefix) && name.ends_with(&suffix) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

fn temporary_output_path(output: &Path, extension: &str) -> PathBuf {
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("video");
    let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    parent.join(format!(
        ".{stem}.blockscope-{}-{id}.{extension}",
        std::process::id()
    ))
}

fn replace_output(temp: &Path, output: &Path) -> Result<(), VideoError> {
    match fs::rename(temp, output) {
        Ok(()) => Ok(()),
        Err(_) if output.exists() => {
            // Windows does not replace an existing destination with rename.
            fs::remove_file(output).map_err(|error| {
                VideoError::temp(format!("cannot replace {}: {error}", output.display()))
            })?;
            fs::rename(temp, output).map_err(|error| {
                VideoError::temp(format!(
                    "cannot move {} to {}: {error}",
                    temp.display(),
                    output.display()
                ))
            })
        }
        Err(error) => Err(VideoError::temp(format!(
            "cannot move {} to {}: {error}",
            temp.display(),
            output.display()
        ))),
    }
}

fn file_size(path: &Path) -> Result<u64, VideoError> {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(|error| VideoError::temp(format!("cannot stat {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::{gif_encode_args, gif_palette_args, mp4_args, remove_stale_output_temps};
    use crate::encoder::VideoSettings;
    use crate::ffmpeg::{Encoder, Quality, ResolvedEncoder, VideoFormat};
    use std::path::Path;

    fn settings(format: VideoFormat, transparent: bool, looped: bool) -> VideoSettings {
        VideoSettings {
            width: 64,
            height: 32,
            fps: 24,
            format,
            quality: Quality::Medium,
            encoder: Encoder::Software,
            looped,
            transparent,
            threads: 8,
        }
    }

    fn strings(args: &[std::ffi::OsString]) -> Vec<String> {
        args.iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    fn resolved(encoder: Encoder) -> ResolvedEncoder {
        ResolvedEncoder::new(encoder, encoder.ffmpeg_name().unwrap_or("gif"))
    }

    #[test]
    fn mp4_args_encode_rgba_to_h264_with_faststart() {
        let args = mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::Software),
            Path::new("out.mp4"),
        );
        let text = strings(&args);
        assert!(text.windows(2).any(|pair| pair == ["-c:v", "libx264"]));
        assert!(text.windows(2).any(|pair| pair == ["-pix_fmt", "yuv420p"]));
        assert!(
            text.windows(2)
                .any(|pair| pair == ["-movflags", "+faststart"])
        );
        assert!(text.windows(2).any(|pair| pair == ["-framerate", "24"]));
        assert!(text.windows(2).any(|pair| pair == ["-crf", "23"]));
        assert!(text.windows(2).any(|pair| pair == ["-threads", "2"]));
    }

    #[test]
    fn hardware_encoders_use_their_own_quality_flags() {
        let nvenc = strings(&mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::Nvenc),
            Path::new("out.mp4"),
        ));
        assert!(nvenc.windows(2).any(|pair| pair == ["-c:v", "h264_nvenc"]));
        assert!(nvenc.windows(2).any(|pair| pair == ["-cq", "23"]));
        assert!(!nvenc.windows(2).any(|pair| pair == ["-crf", "23"]));
        assert!(!nvenc.iter().any(|arg| arg == "-threads"));

        let qsv = strings(&mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::Qsv),
            Path::new("out.mp4"),
        ));
        assert!(qsv.windows(2).any(|pair| pair == ["-global_quality", "23"]));

        let amf = strings(&mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::Amf),
            Path::new("out.mp4"),
        ));
        assert!(amf.windows(2).any(|pair| pair == ["-quality", "balanced"]));
        assert!(amf.windows(2).any(|pair| pair == ["-qp_i", "23"]));

        let videotoolbox = strings(&mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::VideoToolbox),
            Path::new("out.mp4"),
        ));
        assert!(videotoolbox.windows(2).any(|pair| pair == ["-q:v", "65"]));

        let vaapi = strings(&mp4_args(
            &settings(VideoFormat::Mp4, false, true),
            resolved(Encoder::Vaapi),
            Path::new("out.mp4"),
        ));
        assert!(
            vaapi
                .windows(2)
                .any(|pair| pair == ["-vaapi_device", "/dev/dri/renderD128"])
        );
        assert!(
            vaapi
                .windows(2)
                .any(|pair| pair == ["-vf", "format=nv12,hwupload"])
        );
        assert!(!vaapi.windows(2).any(|pair| pair == ["-pix_fmt", "yuv420p"]));
    }

    #[test]
    fn gif_args_reserve_transparency_and_use_loop_flag() {
        let settings = settings(VideoFormat::Gif, true, true);
        let palette = gif_palette_args(
            &settings,
            Path::new("frames.rgba"),
            Path::new("palette.png"),
        );
        assert!(
            palette
                .iter()
                .any(|arg| arg.to_string_lossy().contains("reserve_transparent=1"))
        );
        let encode = gif_encode_args(
            &settings,
            Path::new("frames.rgba"),
            Path::new("palette.png"),
            Path::new("out.gif"),
        );
        let text: Vec<String> = encode
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(text.windows(2).any(|pair| pair == ["-loop", "0"]));
        assert!(text.iter().any(|arg| arg.contains("alpha_threshold=128")));
    }

    #[test]
    fn stale_output_temps_for_the_same_stem_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("clip.mp4");
        let stale = dir.path().join(".clip.blockscope-42-0.mp4");
        let stale_gif = dir.path().join(".clip.blockscope-42-1.gif");
        let unrelated = dir.path().join(".other.blockscope-42-0.mp4");
        for path in [&stale, &stale_gif, &unrelated] {
            std::fs::write(path, b"x").unwrap();
        }

        remove_stale_output_temps(&output, "mp4");
        assert!(!stale.exists());
        assert!(stale_gif.exists());
        assert!(unrelated.exists());

        remove_stale_output_temps(&output, "gif");
        assert!(!stale_gif.exists());
    }

    #[test]
    fn validation_rejects_odd_mp4_and_transparent_mp4() {
        let mut settings = settings(VideoFormat::Mp4, false, true);
        settings.width = 65;
        assert!(settings.validate().is_err());
        settings.width = 64;
        settings.transparent = true;
        assert!(settings.validate().is_err());
    }
}
