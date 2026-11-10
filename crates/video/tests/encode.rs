use blockscope_video::{
    Encoder, Ffmpeg, Quality, VideoEncoder, VideoErrorCode, VideoFormat, VideoSettings,
};
use std::path::{Path, PathBuf};

fn settings(format: VideoFormat, transparent: bool) -> VideoSettings {
    VideoSettings {
        width: 16,
        height: 16,
        fps: 10,
        format,
        quality: Quality::Medium,
        encoder: Encoder::Software,
        looped: true,
        transparent,
        threads: 4,
    }
}

fn frame(red: u8) -> Vec<u8> {
    [red, 40, 80, 255].repeat(16 * 16)
}

fn discover() -> Option<Ffmpeg> {
    let path = std::env::var_os("BLOCKSCOPE_FFMPEG").map(PathBuf::from);
    match Ffmpeg::discover(path.as_deref()) {
        Ok(ffmpeg) => Some(ffmpeg),
        Err(_) => {
            eprintln!("skipping ffmpeg integration test: ffmpeg is not installed");
            None
        }
    }
}

#[test]
fn missing_ffmpeg_is_reported_with_a_stable_code() {
    let error = Ffmpeg::discover(Some(Path::new("definitely-not-ffmpeg")))
        .expect_err("missing executable must fail");
    assert_eq!(error.code, VideoErrorCode::FfmpegNotFound);
}

#[test]
fn encodes_mp4_and_gif_when_ffmpeg_is_available() {
    let Some(ffmpeg) = discover() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();

    let mp4_path = dir.path().join("out.mp4");
    let mut mp4 = VideoEncoder::start(&ffmpeg, &settings(VideoFormat::Mp4, false), &mp4_path)
        .expect("start mp4 encoder");
    for index in 0..4 {
        mp4.write_frame(&frame(index * 50)).unwrap();
    }
    let mp4_bytes = mp4.finish().unwrap();
    assert!(mp4_bytes > 0);
    let header = std::fs::read(&mp4_path).unwrap();
    assert_eq!(&header[4..8], b"ftyp");

    let gif_path = dir.path().join("out.gif");
    let mut gif = VideoEncoder::start(&ffmpeg, &settings(VideoFormat::Gif, true), &gif_path)
        .expect("start gif encoder");
    for index in 0..4 {
        gif.write_frame(&frame(index * 50)).unwrap();
    }
    let gif_bytes = gif.finish().unwrap();
    assert!(gif_bytes > 0);
    let header = std::fs::read(&gif_path).unwrap();
    assert!(header.starts_with(b"GIF8"));
}

#[test]
fn auto_encoder_falls_back_or_uses_hardware_at_the_target_size() {
    let Some(ffmpeg) = discover() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auto.mp4");
    let mut settings = settings(VideoFormat::Mp4, false);
    settings.width = 256;
    settings.height = 256;
    settings.encoder = Encoder::Auto;

    let resolved = ffmpeg
        .resolve_encoder(
            settings.encoder,
            settings.format,
            settings.width,
            settings.height,
        )
        .expect("auto resolution must not fail");
    let mut encoder = VideoEncoder::start(&ffmpeg, &settings, &path).expect("start auto encoder");
    assert_eq!(encoder.encoder_name(), resolved.name());
    let pixels = 256 * 256;
    for index in 0..2 {
        encoder
            .write_frame(&[index * 40, 40, 80, 255].repeat(pixels))
            .unwrap();
    }
    let bytes = encoder.finish().unwrap();
    assert!(bytes > 0);
    let header = std::fs::read(&path).unwrap();
    assert_eq!(&header[4..8], b"ftyp");
}
