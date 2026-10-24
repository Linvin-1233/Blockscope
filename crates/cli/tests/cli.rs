mod support;

use litematic_mesh::MeshStatistics;
use litematic_render::RenderStatistics;
use litematic_scene::ManifestStatistics;
use std::fs;
use std::io::Write;
use std::process::{Command, Output};
use support::*;
use tempfile::NamedTempFile;

fn write_fixture(bytes: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    file
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_renderd"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn inspect_json_prints_manifest() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["inspect", path, "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\"schemaVersion\": 1"));
    assert!(stdout.contains("\"sceneHash\""));
    assert!(stdout.contains("\"minecraft:stone\": 1"));
}

#[test]
fn inspect_human_summary_reports_volume() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["inspect", path]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("volume: 2 (air 1, non-air 1)"));
    assert!(stdout.contains("scene hash:"));
}

#[test]
fn validate_reports_ok() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["validate", path]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("OK:"));
    assert!(stdout.contains("regions: 1"));
    assert!(stdout.contains("volume: 2"));
}

#[test]
fn stats_json_parses() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["stats", path, "--json"]);

    assert!(output.status.success());
    let stats: ManifestStatistics = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stats.volume, 2);
    assert_eq!(stats.air, 1);
    assert_eq!(stats.non_air, 1);
    assert_eq!(stats.blocks.get("minecraft:stone"), Some(&1));
}

#[test]
fn mesh_stats_prints_counts() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["mesh", path]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("quads: 6"));
    assert!(stdout.contains("pages: 1"));
    assert!(stdout.contains("unknown blocks: 0"));
}

#[test]
fn mesh_json_parses() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["mesh", path, "--json"]);

    assert!(output.status.success());
    let stats: MeshStatistics = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stats.quads, 6);
    assert_eq!(stats.pages, 1);
    assert_eq!(stats.blocks_visited, 2);
    assert_eq!(stats.blocks_rendered, 1);
}

#[test]
fn mesh_exports_obj_and_ply() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let obj = dir.path().join("mesh.obj");
    let ply = dir.path().join("mesh.ply");

    let output = run(&[
        "mesh",
        path,
        "--obj",
        obj.to_str().unwrap(),
        "--ply",
        ply.to_str().unwrap(),
    ]);

    assert!(output.status.success());

    let obj_text = fs::read_to_string(&obj).unwrap();
    assert!(obj_text.starts_with("# litematic mesh"));
    assert!(obj_text.contains("\nf "));

    let ply_text = fs::read_to_string(&ply).unwrap();
    assert!(ply_text.starts_with("ply\n"));
    assert!(ply_text.contains("element vertex 24"));
    assert!(ply_text.contains("element face 6"));
}

#[test]
fn render_writes_png() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("render.png");

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--width",
        "64",
        "--height",
        "64",
    ]);

    assert!(output.status.success());
    let bytes = fs::read(&output_path).unwrap();
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("output:"));
    assert!(stdout.contains("triangles:"));
}

#[test]
fn render_multi_view_writes_two_files() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("render.png");

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--views",
        "45,225",
        "--width",
        "64",
        "--height",
        "64",
        "--supersample",
        "1",
    ]);

    assert!(output.status.success());
    assert!(dir.path().join("render-45.png").exists());
    assert!(dir.path().join("render-225.png").exists());
}

#[test]
fn render_json_parses() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("render.png");

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--width",
        "64",
        "--height",
        "64",
        "--json",
    ]);

    assert!(output.status.success());
    let stats: RenderStatistics = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stats.width, 64);
    assert_eq!(stats.height, 64);
    assert_eq!(stats.supersample, 2);
    assert_eq!(stats.internal_width, 128);
    assert!(stats.triangles > 0);
}

#[test]
fn render_rejects_invalid_options() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("render.png");

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--background",
        "not-a-color",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("INVALID_RENDER_OPTIONS")
    );

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--lighting",
        "unknown",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("INVALID_RENDER_OPTIONS")
    );
}

#[test]
fn bench_json_parses() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&[
        "bench",
        path,
        "--width",
        "64",
        "--height",
        "64",
        "--supersample",
        "1",
        "--iterations",
        "1",
        "--json",
    ]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["iterations"], 1);
    assert!(report["renderMs"]["median"].as_f64().unwrap() >= 0.0);
    assert!(report["renderStatistics"]["triangles"].as_u64().unwrap() > 0);
    assert!(report["pngBytes"].as_u64().unwrap() > 0);
}

#[test]
fn bench_rejects_zero_iterations() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["bench", path, "--iterations", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("INVALID_RENDER_OPTIONS")
    );
}

#[test]
fn invalid_file_exits_with_input_error() {
    let file = write_fixture(&[0, 1, 2]);
    let path = file.path().to_str().unwrap();
    let output = run(&["validate", path]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("INVALID_NBT"));
}

#[test]
fn missing_file_exits_with_input_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.litematic");
    let output = run(&["validate", path.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("IO_ERROR"));
}
