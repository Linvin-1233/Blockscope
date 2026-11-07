mod support;

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn start_with(args: &[&str]) -> (Child, ChildStdin, Lines<BufReader<ChildStdout>>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_blockscope"))
        .arg("serve")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn blockscope serve");

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    (child, stdin, BufReader::new(stdout).lines())
}

fn read_json(lines: &mut Lines<BufReader<ChildStdout>>) -> Value {
    let line = lines
        .next()
        .expect("worker closed stdout")
        .expect("read protocol line");
    serde_json::from_str(&line).expect("protocol line is JSON")
}

fn send(stdin: &mut ChildStdin, value: &Value) {
    writeln!(stdin, "{value}").unwrap();
    stdin.flush().unwrap();
}

fn fixture(dir: &std::path::Path) -> std::path::PathBuf {
    let region = support::region_with_palette(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        None,
        &[
            support::palette_entry("minecraft:air"),
            support::palette_entry("minecraft:stone"),
        ],
        &[1],
    );
    let bytes = support::litematic(
        7,
        None,
        3700,
        &support::metadata("serve-video", [1, 1, 1], 1, 1, 1),
        &[region],
    );
    let path = dir.join("model.litematic");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn ffmpeg() -> Option<String> {
    if let Ok(path) = std::env::var("BLOCKSCOPE_FFMPEG") {
        return Some(path);
    }
    std::env::var_os("BLOCKSCOPE_FFMPEG_TESTS").map(|_| "ffmpeg".to_string())
}

fn animation() -> Value {
    json!({
        "keyframes": [
            { "time": 0.0, "yaw": 45.0, "elevation": 30.0 },
            { "time": 0.2, "yaw": 50.0, "elevation": 30.0 }
        ],
        "duration": 0.2,
        "fps": 10,
        "loop": true,
        "easing": "linear"
    })
}

fn shutdown(mut child: Child, mut stdin: ChildStdin, mut lines: Lines<BufReader<ChildStdout>>) {
    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_reports_video_capabilities_and_unavailable_ffmpeg() {
    let (child, stdin, mut lines) = start_with(&["--ffmpeg", "definitely-not-ffmpeg"]);
    let hello = read_json(&mut lines);
    assert_eq!(hello["protocolVersion"], 4);
    assert_eq!(hello["video"]["available"], false);
    assert_eq!(hello["video"]["formats"], json!([]));
    assert_eq!(hello["video"]["encoders"], json!([]));
    assert!(
        hello["video"]["reason"]
            .as_str()
            .unwrap()
            .contains("FFMPEG")
    );

    shutdown(child, stdin, lines);
}

#[test]
fn serve_rejects_png_with_animation_before_reading_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("out");
    let (child, mut stdin, mut lines) = start_with(&[]);
    let _hello = read_json(&mut lines);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "png-animation",
            "input": { "path": dir.path().join("missing.litematic").display().to_string() },
            "format": "png",
            "animation": animation(),
            "outputDir": out_dir.display().to_string(),
        }),
    );
    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "ANIMATION_INVALID");
    assert!(
        !result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("missing.litematic")
    );

    shutdown(child, stdin, lines);
}

#[test]
fn serve_rejects_missing_animation_and_views_before_reading_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let out_dir = dir.path().join("out");
    let (child, mut stdin, mut lines) = start_with(&[]);
    let _hello = read_json(&mut lines);

    let base = json!({
        "type": "render",
        "id": "bad-video",
        "input": { "path": dir.path().join("missing.litematic").display().to_string() },
        "format": "mp4",
        "width": 128,
        "height": 128,
        "outputDir": out_dir.display().to_string(),
    });

    send(&mut stdin, &base);
    let missing = read_json(&mut lines);
    assert_eq!(missing["ok"], false, "{missing}");
    assert_eq!(missing["error"]["code"], "ANIMATION_INVALID");

    let mut with_views = base.clone();
    with_views["animation"] = animation();
    with_views["views"] = json!([45.0]);
    send(&mut stdin, &with_views);
    let views = read_json(&mut lines);
    assert_eq!(views["ok"], false, "{views}");
    assert_eq!(views["error"]["code"], "INVALID_RENDER_OPTIONS");

    shutdown(child, stdin, lines);
}

#[test]
fn serve_rejects_transparent_mp4_before_looking_for_ffmpeg() {
    let dir = tempfile::tempdir().unwrap();
    let (child, mut stdin, mut lines) = start_with(&["--ffmpeg", "definitely-not-ffmpeg"]);
    let _hello = read_json(&mut lines);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "transparent-mp4",
            "input": { "path": dir.path().join("missing.litematic").display().to_string() },
            "format": "mp4",
            "background": "transparent",
            "width": 128,
            "height": 128,
            "animation": animation(),
            "outputDir": dir.path().join("out").display().to_string(),
        }),
    );
    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "ANIMATION_INVALID");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("alpha")
    );

    shutdown(child, stdin, lines);
}

#[test]
fn serve_renders_mp4_and_gif_when_ffmpeg_is_available() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipping ffmpeg service test: set BLOCKSCOPE_FFMPEG");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (child, mut stdin, mut lines) = start_with(&["--ffmpeg", &ffmpeg]);
    let hello = read_json(&mut lines);
    assert_eq!(hello["video"]["available"], true, "{hello}");
    assert!(hello["video"]["ffmpegVersion"].is_string());

    let request = |id: &str, format: &str, background: &str| {
        json!({
            "type": "render",
            "id": id,
            "input": { "path": input.display().to_string() },
            "format": format,
            "background": background,
            "width": 128,
            "height": 128,
            "supersample": 1,
            "encoder": "software",
            "quality": "medium",
            "animation": animation(),
            "outputDir": out_dir.display().to_string(),
        })
    };

    send(&mut stdin, &request("clip-mp4", "mp4", "#101010"));
    let mp4 = read_json(&mut lines);
    assert_eq!(mp4["ok"], true, "{mp4}");
    let output = &mp4["outputs"][0];
    assert_eq!(output["kind"], "video");
    assert_eq!(output["format"], "mp4");
    assert_eq!(output["frames"], 2);
    assert_eq!(output["fps"], 10);
    assert_eq!(output["durationSeconds"], 0.2);
    assert_eq!(output["encoder"], "libx264");
    assert!(output["renderMs"].as_f64().unwrap() >= 0.0);
    assert!(output["encodeMs"].as_f64().unwrap() >= 0.0);
    let path = output["path"].as_str().unwrap();
    assert!(path.ends_with(".mp4"));
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
    assert!(mp4["timings"]["encodeMs"].as_f64().unwrap() >= 0.0);

    send(&mut stdin, &request("clip-gif", "gif", "transparent"));
    let gif = read_json(&mut lines);
    assert_eq!(gif["ok"], true, "{gif}");
    let output = &gif["outputs"][0];
    assert_eq!(output["kind"], "video");
    assert_eq!(output["format"], "gif");
    let bytes = std::fs::read(output["path"].as_str().unwrap()).unwrap();
    assert!(bytes.starts_with(b"GIF8"));

    shutdown(child, stdin, lines);
}
