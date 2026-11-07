mod support;

use support::{basic_fixture, run, write_fixture};

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn animate_rejects_invalid_keyframes_before_looking_for_ffmpeg() {
    let file = write_fixture(&basic_fixture());
    let output_path = tempfile::NamedTempFile::new().unwrap();
    let output = run(&[
        "animate",
        file.path().to_str().unwrap(),
        "--output",
        output_path.path().to_str().unwrap(),
        "--keyframe",
        "1:0:0",
        "--keyframe",
        "0:0:0",
        "--ffmpeg",
        "definitely-not-ffmpeg",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("ANIMATION_INVALID"), "{text}");
    assert!(!text.contains("FFMPEG_NOT_FOUND"), "{text}");
}

#[test]
fn animate_rejects_missing_ffmpeg_before_reading_the_input() {
    let output_path = tempfile::NamedTempFile::new().unwrap();
    let output = run(&[
        "animate",
        "does-not-exist.litematic",
        "--output",
        output_path.path().to_str().unwrap(),
        "--keyframe",
        "0:45:30",
        "--keyframe",
        "1:405:30",
        "--ffmpeg",
        "definitely-not-ffmpeg",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("FFMPEG_NOT_FOUND"), "{text}");
    assert!(!text.contains("does-not-exist.litematic"), "{text}");
}

#[test]
fn animate_rejects_transparent_mp4_before_looking_for_ffmpeg() {
    let file = write_fixture(&basic_fixture());
    let output_path = tempfile::NamedTempFile::new().unwrap();
    let output = run(&[
        "animate",
        file.path().to_str().unwrap(),
        "--output",
        output_path.path().to_str().unwrap(),
        "--keyframe",
        "0:45:30",
        "--keyframe",
        "1:405:30",
        "--background",
        "transparent",
        "--ffmpeg",
        "definitely-not-ffmpeg",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("ANIMATION_INVALID"), "{text}");
    assert!(text.contains("alpha"), "{text}");
}

#[test]
fn animate_rejects_odd_mp4_dimensions() {
    let file = write_fixture(&basic_fixture());
    let output_path = tempfile::NamedTempFile::new().unwrap();
    let output = run(&[
        "animate",
        file.path().to_str().unwrap(),
        "--output",
        output_path.path().to_str().unwrap(),
        "--keyframe",
        "0:45:30",
        "--keyframe",
        "1:405:30",
        "--width",
        "65",
        "--ffmpeg",
        "definitely-not-ffmpeg",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("even width and height"), "{text}");
}

#[test]
fn animate_rejects_render_only_camera_flags() {
    let file = write_fixture(&basic_fixture());
    let dir = tempfile::tempdir().unwrap();
    let output = run(&[
        "animate",
        file.path().to_str().unwrap(),
        "--output",
        dir.path().join("out.mp4").to_str().unwrap(),
        "--keyframe",
        "0:45:30",
        "--keyframe",
        "1:405:30",
        "--yaw",
        "45",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("does not accept"), "{text}");
}

#[test]
fn animate_rejects_format_extension_mismatch() {
    let file = write_fixture(&basic_fixture());
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("out.mp4");
    let output = run(&[
        "animate",
        file.path().to_str().unwrap(),
        "--output",
        output_path.to_str().unwrap(),
        "--format",
        "gif",
        "--keyframe",
        "0:45:30",
        "--keyframe",
        "1:405:30",
        "--ffmpeg",
        "definitely-not-ffmpeg",
    ]);

    assert_eq!(output.status.code(), Some(2));
    let text = stderr(&output);
    assert!(text.contains("does not match"), "{text}");
}
