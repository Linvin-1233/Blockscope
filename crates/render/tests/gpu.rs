#![cfg(feature = "gpu")]

mod support;

use litematic_material::{AssetSource, MaterialSet, TextureFilter};
use litematic_mesh::{ColorBlockCatalog, Mesh, MeshLimits, Rgba, build_mesh};
use litematic_render::{
    Background, RenderBackend, RenderErrorCode, RenderOptions, RenderOutput, render_mesh,
    render_mesh_with_textures,
};
use litematic_scene::Scene;
use support::*;

fn build(scene: &Scene) -> Mesh {
    build_mesh(scene, &ColorBlockCatalog::default(), &MeshLimits::default()).unwrap()
}

fn stone_scene() -> Scene {
    scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 2, 2],
        &[
            "minecraft:air",
            "minecraft:stone",
            "minecraft:oak_leaves",
            "minecraft:water",
        ],
        &[1, 2, 3, 1, 2, 3, 1, 2],
    )])
}

fn options(backend: RenderBackend) -> RenderOptions {
    RenderOptions {
        width: 96,
        height: 96,
        supersample: 1,
        threads: 1,
        backend,
        ..Default::default()
    }
}

fn compare(cpu: &RenderOutput, gpu: &RenderOutput) {
    assert_eq!(cpu.width, gpu.width);
    assert_eq!(cpu.height, gpu.height);
    let mut max_delta = 0i32;
    let mut differing = 0u64;
    for (left, right) in cpu.rgba.chunks_exact(4).zip(gpu.rgba.chunks_exact(4)) {
        let delta = left
            .iter()
            .zip(right)
            .map(|(&a, &b)| (i32::from(a) - i32::from(b)).abs())
            .max()
            .unwrap_or(0);
        max_delta = max_delta.max(delta);
        if delta > 0 {
            differing += 1;
        }
    }
    let pixels = u64::from(cpu.width) * u64::from(cpu.height);
    let mut alpha_mismatch = 0u64;
    let mut cpu_only = 0u64;
    let mut gpu_only = 0u64;
    for (left, right) in cpu.rgba.chunks_exact(4).zip(gpu.rgba.chunks_exact(4)) {
        match (left[3] > 0, right[3] > 0) {
            (true, true) => {}
            (true, false) => cpu_only += 1,
            (false, true) => gpu_only += 1,
            (false, false) => alpha_mismatch += 1,
        }
    }
    eprintln!(
        "max_delta={max_delta} differing={differing}/{pixels} cpu_only={cpu_only} gpu_only={gpu_only} both_transparent={alpha_mismatch}"
    );
    assert!(
        max_delta <= 3,
        "max channel delta {max_delta} exceeds tolerance"
    );
    assert!(
        differing * 100 <= pixels * 3,
        "{differing} of {pixels} pixels differ"
    );
}

#[test]
fn gpu_matches_cpu_for_opaque_cutout_and_translucent_geometry() {
    let scene = stone_scene();
    let mesh = build(&scene);
    let cpu = render_mesh(&scene, &mesh, &options(RenderBackend::Cpu)).unwrap();
    let gpu = match render_mesh(&scene, &mesh, &options(RenderBackend::Gpu)) {
        Ok(output) => output,
        Err(error) if error.code == RenderErrorCode::BackendUnavailable => {
            eprintln!("skipping GPU parity test: {error}");
            return;
        }
        Err(error) => panic!("GPU render failed: {error}"),
    };

    assert_eq!(gpu.statistics.backend_used, RenderBackend::Gpu);
    compare(&cpu, &gpu);
}

#[test]
fn gpu_textured_matches_cpu() {
    let pack = tempfile::tempdir().unwrap();
    support::pack(pack.path());
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:stone"],
        &[0],
    )]);
    let mut source = AssetSource::new();
    source.add_dir(pack.path()).unwrap();
    let materials = MaterialSet::open(source).unwrap();
    let mesh = build_mesh(&scene, &materials, &MeshLimits::default()).unwrap();
    let textures = materials.textures();

    for filter in [TextureFilter::Nearest, TextureFilter::Linear] {
        let mut cpu_options = options(RenderBackend::Cpu);
        cpu_options.texture_filter = filter;
        let mut gpu_options = cpu_options.clone();
        gpu_options.backend = RenderBackend::Gpu;
        let cpu = render_mesh_with_textures(&scene, &mesh, &cpu_options, &textures).unwrap();
        let gpu = match render_mesh_with_textures(&scene, &mesh, &gpu_options, &textures) {
            Ok(output) => output,
            Err(error) if error.code == RenderErrorCode::BackendUnavailable => {
                eprintln!("skipping textured GPU parity test: {error}");
                return;
            }
            Err(error) => panic!("GPU textured render failed: {error}"),
        };
        assert_eq!(gpu.statistics.backend_used, RenderBackend::Gpu);
        compare(&cpu, &gpu);
    }
}

#[test]
fn gpu_matches_cpu_over_solid_backgrounds() {
    let scene = stone_scene();
    let mesh = build(&scene);
    for color in [Rgba::new(10, 20, 30, 255), Rgba::new(10, 20, 30, 128)] {
        let mut cpu_options = options(RenderBackend::Cpu);
        cpu_options.background = Background::Solid(color);
        let mut gpu_options = cpu_options.clone();
        gpu_options.backend = RenderBackend::Gpu;
        let cpu = render_mesh(&scene, &mesh, &cpu_options).unwrap();
        let gpu = match render_mesh(&scene, &mesh, &gpu_options) {
            Ok(output) => output,
            Err(error) if error.code == RenderErrorCode::BackendUnavailable => {
                eprintln!("skipping GPU background test: {error}");
                return;
            }
            Err(error) => panic!("GPU background render failed: {error}"),
        };
        compare(&cpu, &gpu);
    }
}

#[test]
fn auto_keeps_small_meshes_on_the_cpu() {
    let scene = stone_scene();
    let mesh = build(&scene);
    let output = render_mesh(&scene, &mesh, &options(RenderBackend::Auto)).unwrap();
    assert_eq!(output.statistics.backend_used, RenderBackend::Cpu);
}

#[test]
fn explicit_gpu_rejects_streaming() {
    let scene = stone_scene();
    let mesh = build(&scene);
    let mut options = options(RenderBackend::Gpu);
    options.force_streaming = true;
    let error = render_mesh(&scene, &mesh, &options).unwrap_err();
    assert_eq!(error.code, RenderErrorCode::InvalidOptions);
}
