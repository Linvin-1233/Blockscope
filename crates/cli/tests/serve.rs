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

fn start() -> (Child, ChildStdin, Lines<BufReader<ChildStdout>>) {
    start_with(&[])
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
    fixture_named(dir, "serve")
}

fn fixture_named(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
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
        &support::metadata(name, [1, 1, 1], 1, 1, 1),
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
    assert_eq!(hello["protocolVersion"], 3);
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
fn serve_reuses_cached_scene_and_mesh() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    let request = |id: &str| {
        json!({
            "type": "render",
            "id": id,
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        })
    };

    send(&mut stdin, &request("cache-1"));
    let first = read_json(&mut lines);
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["cacheHit"], false);

    send(&mut stdin, &request("cache-2"));
    let second = read_json(&mut lines);
    assert_eq!(second["ok"], true, "{second}");
    assert_eq!(second["cacheHit"], true);
    assert_eq!(second["timings"]["meshMs"], 0.0);
    assert_eq!(second["timings"]["parseMs"], 0.0);

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_cache_evicts_to_stay_within_its_byte_budget() {
    let dir = tempfile::tempdir().unwrap();
    let first_input = fixture(dir.path());
    let second_dir = dir.path().join("second");
    std::fs::create_dir(&second_dir).unwrap();
    let second_input = fixture_named(&second_dir, "serve-second");
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start_with(&["--cache", "2", "--cache-bytes", "1"]);
    let _hello = read_json(&mut lines);

    let request = |id: &str, input: &std::path::Path| {
        json!({
            "type": "render",
            "id": id,
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        })
    };

    send(&mut stdin, &request("byte-1", &first_input));
    assert_eq!(read_json(&mut lines)["cacheHit"], false);
    send(&mut stdin, &request("byte-1-repeat", &first_input));
    assert_eq!(read_json(&mut lines)["cacheHit"], true);
    send(&mut stdin, &request("byte-2", &second_input));
    assert_eq!(read_json(&mut lines)["cacheHit"], false);
    send(&mut stdin, &request("byte-3", &first_input));
    assert_eq!(read_json(&mut lines)["cacheHit"], false);

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_cache_hit_with_hash_does_not_read_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");
    let hash = litematic_scene::sha256(&std::fs::read(&input).unwrap()).to_hex();

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    let request = |id: &str| {
        json!({
            "type": "render",
            "id": id,
            "input": { "path": input.display().to_string(), "hash": hash },
            "views": [45],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        })
    };

    send(&mut stdin, &request("hash-1"));
    let first = read_json(&mut lines);
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["cacheHit"], false);

    std::fs::remove_file(&input).unwrap();
    send(&mut stdin, &request("hash-2"));
    let second = read_json(&mut lines);
    assert_eq!(second["ok"], true, "{second}");
    assert_eq!(second["cacheHit"], true);
    assert_eq!(second["timings"]["parseMs"], 0.0);
    assert_eq!(second["timings"]["meshMs"], 0.0);

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_multi_view_keeps_output_order() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);
    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "views",
            "input": { "path": input.display().to_string() },
            "views": [45, 225],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "threads": 2,
            "outputDir": out_dir.display().to_string(),
        }),
    );

    let result = read_json(&mut lines);
    assert_eq!(result["ok"], true, "{result}");
    let outputs = result["outputs"].as_array().unwrap();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0]["yaw"], 45.0);
    assert_eq!(outputs[1]["yaw"], 225.0);
    assert!(outputs[0]["path"].as_str().unwrap().ends_with("-45.png"));
    assert!(outputs[1]["path"].as_str().unwrap().ends_with("-225.png"));

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_reports_the_effective_cache_budget() {
    let (mut child, mut stdin, mut lines) = start_with(&["--cache", "1", "--cache-bytes", "12345"]);
    let hello = read_json(&mut lines);

    assert_eq!(hello["cacheBytes"], 12345);

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_rejects_overlong_request_lines_without_exiting() {
    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    let overlong = "x".repeat(1024 * 1024 + 1);
    writeln!(stdin, "{overlong}").unwrap();
    stdin.flush().unwrap();

    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "INVALID_REQUEST");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_rejects_too_many_views_before_reading_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("missing.litematic");
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);
    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "too-many-views",
            "input": { "path": input.display().to_string() },
            "views": vec![45.0; 65],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        }),
    );

    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "INVALID_RENDER_OPTIONS");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_rejects_invalid_render_options_before_reading_the_input() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("missing.litematic");
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);
    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "bad-size",
            "input": { "path": input.display().to_string() },
            "width": 0,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        }),
    );

    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "INVALID_RENDER_OPTIONS");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_rejects_oversized_thread_requests() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "threads",
            "input": { "path": input.display().to_string() },
            "width": 64,
            "height": 64,
            "threads": 100_000,
            "outputDir": out_dir.display().to_string(),
        }),
    );
    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(result["error"]["code"], "INVALID_RENDER_OPTIONS");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
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

#[test]
fn serve_hello_advertises_available_backends() {
    let (mut child, mut stdin, mut lines) = start();
    let hello = read_json(&mut lines);
    let backends: Vec<&str> = hello["backends"]
        .as_array()
        .expect("backends array")
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert!(backends.contains(&"cpu"));
    assert!(backends.contains(&"auto"));
    assert_eq!(backends.contains(&"gpu"), cfg!(feature = "gpu"));

    send(&mut stdin, &json!({ "type": "shutdown" }));
    let _bye = read_json(&mut lines);
    assert!(child.wait().unwrap().success());
}

#[test]
fn serve_rejects_an_unknown_backend() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "bad-backend",
            "input": { "path": input.display().to_string() },
            "backend": "quantum",
            "outputDir": dir.path().join("out").display().to_string(),
        }),
    );
    let result = read_json(&mut lines);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "INVALID_RENDER_OPTIONS");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("unknown backend 'quantum'")
    );

    send(&mut stdin, &json!({ "type": "shutdown" }));
    let _bye = read_json(&mut lines);
    assert!(child.wait().unwrap().success());
}

#[cfg(feature = "gpu")]
#[test]
fn serve_renders_with_the_gpu_backend() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let output_dir = dir.path().join("gpu-out");
    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "gpu-job",
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "backend": "gpu",
            "outputDir": output_dir.display().to_string(),
        }),
    );
    let result = read_json(&mut lines);
    if result["ok"] == false {
        let code = result["error"]["code"].as_str().unwrap();
        if code == "BACKEND_UNAVAILABLE" {
            eprintln!("skipping GPU serve test: {result}");
        } else {
            panic!("GPU serve render failed: {result}");
        }
    } else {
        assert_eq!(result["statistics"]["backendUsed"], "gpu");
        let path = result["outputs"][0]["path"].as_str().unwrap();
        assert!(
            std::fs::read(path)
                .unwrap()
                .starts_with(&[0x89, b'P', b'N', b'G'])
        );
    }

    send(&mut stdin, &json!({ "type": "shutdown" }));
    let _bye = read_json(&mut lines);
    assert!(child.wait().unwrap().success());
}
