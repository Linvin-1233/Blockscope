mod support;

use serde_json::{Value, json};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Lines, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn start() -> (Child, ChildStdin, Lines<BufReader<ChildStdout>>) {
    start_with(&["--workers", "2", "--threads", "1", "--cache", "1"])
}

fn start_with(args: &[&str]) -> (Child, ChildStdin, Lines<BufReader<ChildStdout>>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_renderd"))
        .arg("supervise")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn renderd supervise");

    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    (child, stdin, BufReader::new(stdout).lines())
}

fn read_json(lines: &mut Lines<BufReader<ChildStdout>>) -> Value {
    let line = lines
        .next()
        .expect("supervisor closed stdout")
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
        &support::metadata("supervise", [1, 1, 1], 1, 1, 1),
        &[region],
    );

    let path = dir.join("model.litematic");
    std::fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn supervisor_runs_jobs_on_multiple_workers() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start();
    let hello = read_json(&mut lines);
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["workers"], 2);
    assert!(hello["queueLimit"].as_u64().unwrap() >= 1);

    for id in ["job-a", "job-b"] {
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
    }

    let mut seen = HashSet::new();
    for _ in 0..2 {
        let result = read_json(&mut lines);
        assert_eq!(result["ok"], true, "{result}");
        seen.insert(result["id"].as_str().unwrap().to_string());
    }
    assert_eq!(seen.len(), 2);

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn supervisor_sticks_repeated_inputs_to_the_cached_worker() {
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

    send(&mut stdin, &request("sticky-1"));
    let first = read_json(&mut lines);
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(first["cacheHit"], false);

    send(&mut stdin, &request("sticky-2"));
    let second = read_json(&mut lines);
    assert_eq!(second["ok"], true, "{second}");
    assert_eq!(second["cacheHit"], true, "{second}");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn supervisor_times_out_and_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start_with(&[
        "--workers",
        "1",
        "--threads",
        "1",
        "--cache",
        "0",
        "--timeout",
        "1",
    ]);
    let hello = read_json(&mut lines);
    assert_eq!(hello["timeoutSeconds"], 1);

    // One thread at 2048 supersample 2 needs several seconds, so a one second
    // deadline fires twice: the retry, then the final TIMEOUT.
    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "slow",
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 2048,
            "height": 2048,
            "supersample": 2,
            "threads": 1,
            "outputDir": out_dir.display().to_string(),
        }),
    );
    let timed_out = read_json(&mut lines);
    assert_eq!(timed_out["ok"], false, "{timed_out}");
    assert_eq!(timed_out["error"]["code"], "TIMEOUT");

    // The worker must have been restarted and still serve jobs.
    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "after-timeout",
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 64,
            "height": 64,
            "supersample": 1,
            "outputDir": out_dir.display().to_string(),
        }),
    );
    let recovered = read_json(&mut lines);
    assert_eq!(recovered["ok"], true, "{recovered}");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn supervisor_enforces_the_memory_ceiling() {
    let dir = tempfile::tempdir().unwrap();
    let input = fixture(dir.path());
    let out_dir = dir.path().join("out");

    let (mut child, mut stdin, mut lines) = start_with(&[
        "--workers",
        "1",
        "--threads",
        "1",
        "--cache",
        "0",
        "--memory",
        "1",
    ]);
    let hello = read_json(&mut lines);
    assert_eq!(hello["memoryLimitPerWorkerBytes"], 1024 * 1024);

    send(
        &mut stdin,
        &json!({
            "type": "render",
            "id": "hungry",
            "input": { "path": input.display().to_string() },
            "views": [45],
            "width": 2048,
            "height": 2048,
            "supersample": 2,
            "threads": 1,
            "outputDir": out_dir.display().to_string(),
        }),
    );
    let limited = read_json(&mut lines);
    assert_eq!(limited["ok"], false, "{limited}");
    assert_eq!(limited["error"]["code"], "MEMORY_LIMIT");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}

#[test]
fn supervisor_reports_bad_messages_without_exiting() {
    let (mut child, mut stdin, mut lines) = start();
    let _hello = read_json(&mut lines);

    writeln!(stdin, "not json").unwrap();
    stdin.flush().unwrap();
    let invalid = read_json(&mut lines);
    assert_eq!(invalid["ok"], false);
    assert_eq!(invalid["error"]["code"], "INVALID_REQUEST");

    send(&mut stdin, &json!({ "type": "shutdown" }));
    assert_eq!(read_json(&mut lines)["type"], "bye");
    assert!(child.wait().unwrap().success());
}
