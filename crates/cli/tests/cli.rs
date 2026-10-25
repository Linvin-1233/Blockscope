mod support;

use litematic_mesh::MeshStatistics;
use litematic_render::RenderStatistics;
use litematic_scene::ManifestStatistics;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
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

fn write_material_pack(root: &Path) {
    let assets = root.join("assets/minecraft");
    fs::create_dir_all(assets.join("blockstates")).unwrap();
    fs::create_dir_all(assets.join("models/block")).unwrap();
    fs::create_dir_all(assets.join("textures/block")).unwrap();

    fs::write(
        assets.join("blockstates/stone.json"),
        r##"{ "variants": { "": { "model": "block/test_cube" } } }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/cube.json"),
        r##"{
            "elements": [{
                "from": [0, 0, 0],
                "to": [16, 16, 16],
                "faces": {
                    "down":  { "texture": "#all", "cullface": "down" },
                    "up":    { "texture": "#all", "cullface": "up" },
                    "north": { "texture": "#all", "cullface": "north" },
                    "south": { "texture": "#all", "cullface": "south" },
                    "west":  { "texture": "#all", "cullface": "west" },
                    "east":  { "texture": "#all", "cullface": "east" }
                }
            }]
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/cube_all.json"),
        r##"{
            "parent": "block/cube",
            "textures": {
                "down": "#all", "up": "#all", "north": "#all",
                "south": "#all", "west": "#all", "east": "#all"
            }
        }"##,
    )
    .unwrap();
    fs::write(
        assets.join("models/block/test_cube.json"),
        r##"{
            "parent": "block/cube_all",
            "textures": { "all": "block/test_texture" }
        }"##,
    )
    .unwrap();

    let file = fs::File::create(assets.join("textures/block/test_texture.png")).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), 16, 16);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&[200, 40, 40, 255].repeat(16 * 16))
        .unwrap();
    writer.finish().unwrap();
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

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "--texture-filter",
        "magic",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("INVALID_RENDER_OPTIONS")
    );
}

#[test]
fn render_with_assets_writes_textured_png() {
    let pack = tempfile::tempdir().unwrap();
    write_material_pack(pack.path());

    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("textured.png");

    let output = run(&[
        "render",
        path,
        "--assets",
        pack.path().to_str().unwrap(),
        "--output",
        output_path.to_str().unwrap(),
        "--width",
        "64",
        "--height",
        "64",
        "--supersample",
        "1",
    ]);

    assert!(output.status.success());
    assert!(
        fs::read(&output_path)
            .unwrap()
            .starts_with(&[0x89, b'P', b'N', b'G'])
    );

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("materials: pack"));
}

#[test]
fn materials_json_reports_pack_coverage() {
    let pack = tempfile::tempdir().unwrap();
    write_material_pack(pack.path());

    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&[
        "materials",
        path,
        "--assets",
        pack.path().to_str().unwrap(),
        "--json",
    ]);

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["packModels"], 1);
    assert_eq!(report["textured"], 1);
    assert_eq!(report["unknown"], 0);
}

#[test]
fn materials_requires_assets() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["materials", path]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("ASSET_SOURCE_REQUIRED")
    );
}

#[test]
fn missing_client_jar_exits_with_asset_error() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("out.png");

    let output = run(&[
        "render",
        path,
        "--client-jar",
        "does-not-exist.jar",
        "--output",
        output_path.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("ASSET_READ_FAILED")
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

#[test]
fn lod_reduces_the_mesh() {
    let region = support::region_with_palette(
        "main",
        [0, 0, 0],
        [4, 4, 4],
        None,
        &[
            palette_entry("minecraft:air"),
            palette_entry("minecraft:stone"),
        ],
        &[0x5555_5555_5555_5555, 0x5555_5555_5555_5555],
    );
    let bytes = litematic(
        7,
        None,
        3700,
        &metadata("lod", [4, 4, 4], 1, 64, 64),
        &[region],
    );
    let fixture = write_fixture(&bytes);
    let path = fixture.path().to_str().unwrap();

    let plain_output = run(&["mesh", path, "--json"]);
    assert!(
        plain_output.status.success(),
        "{}",
        String::from_utf8_lossy(&plain_output.stderr)
    );
    let plain: MeshStatistics = serde_json::from_slice(&plain_output.stdout).unwrap();

    let lod_output = run(&["mesh", path, "--json", "--lod", "2"]);
    assert!(
        lod_output.status.success(),
        "{}",
        String::from_utf8_lossy(&lod_output.stderr)
    );
    let lod: MeshStatistics = serde_json::from_slice(&lod_output.stdout).unwrap();

    assert!(plain.quads > 0);
    assert!(
        lod.quads < plain.quads,
        "lod quads {} should be below {}",
        lod.quads,
        plain.quads
    );
}

#[test]
fn force_limits_prints_a_warning() {
    let fixture = write_fixture(&basic_fixture());
    let path = fixture.path().to_str().unwrap();

    let forced = run(&["mesh", path, "--json", "--force-limits"]);
    assert!(forced.status.success());
    let stderr = String::from_utf8(forced.stderr).unwrap();
    assert!(
        stderr.contains("WARNING: resource limits are disabled"),
        "{stderr}"
    );
    let stats: MeshStatistics = serde_json::from_slice(&forced.stdout).unwrap();
    assert!(stats.quads > 0);

    let default = run(&["mesh", path, "--json"]);
    assert!(default.status.success());
    assert!(
        default.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&default.stderr)
    );
}
