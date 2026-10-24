mod support;

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn start() -> (Child, ChildStdin, Lines<BufReader<ChildStdout>>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_renderd"))
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn renderd serve");

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
        &support::metadata("serve", [1, 1, 1], 1, 1, 1),
        &[region],
    );

    let path = dir.join("model.litematic");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn serve_renders_two_jobs_and_shuts_down() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let hello = read_json(&mut lines);
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["protocolVersion"], 1);
    assert!(hello["suggestedWorkers"].as_u64().unwrap() >= 1);
    assert!(hello["defaultTier"]["width"].as_u64().unwrap() >= 1);

    for id in ["job-1", "job-2"] {
        send(
            &mut stdin,
            &json!({
                "type": "render",
                "id": id,
                "input": { "path": input.display().to_string() },
                "views": [45],
                "width": 64,
                "height": 64,
                "supersample": 1,
                "outputDir": out_dir.display().to_string(),
            }),
        );

        let result = read_json(&mut lines);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["id"], id);
        assert!(result["sceneHash"].as_str().unwrap().len() == 64);

        let path = result["outputs"][0]["path"].as_str().unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
    }

    send(&mut stdin, &json!({ "type": "shutdown" }));
    let bye = read_json(&mut lines);
    assert_eq!(bye["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_reports_bad_requests_without_exiting() {
    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    writeln!(stdin, "not json").unwrap();
    stdin.flush().unwrap();
    let malformed = read_json(&mut lines);
    assert_eq!(malformed["ok"], false);
    assert_eq!(malformed["error"]["code"], "INVALID_REQUEST");

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "bad id",
            "input": { "path": "/does/not/exist.litematic" },
            "outputDir": "/tmp/out"
        }),
    );
    let invalid = read_json(&mut lines);
    assert_eq!(invalid["ok"], false);
    assert_eq!(invalid["error"]["code"], "INVALID_REQUEST");
    assert_eq!(invalid["id"], "bad id");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}
