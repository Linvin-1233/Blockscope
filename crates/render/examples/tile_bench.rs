//! Compare single-threaded and parallel tile rasterization on a synthetic scene.
//!
//! Usage: `cargo run --release -p litematic-render --example tile_bench`
//! Options: `--width N`, `--height N`, `--supersample N`, `--iterations N`,
//! `--backend cpu|gpu` (GPU requires `--features gpu`).

use litematic_mesh::{ColorBlockCatalog, MeshLimits, build_mesh};
use litematic_render::{RenderBackend, RenderOptions, render_mesh};
use litematic_scene::{BlockStateKey, PackedIndices, Region, Scene, SceneMetadata};
use std::time::Instant;

fn main() {
    let mut width = 1024;
    let mut height = 1024;
    let mut supersample = 2;
    let mut iterations = 3;
    let mut backend = RenderBackend::Cpu;
    parse_args(
        &mut width,
        &mut height,
        &mut supersample,
        &mut iterations,
        &mut backend,
    );
    if backend == RenderBackend::Gpu && !cfg!(feature = "gpu") {
        eprintln!("gpu backend requires --features gpu");
        std::process::exit(2);
    }
    println!("backend: {}", backend.as_str());

    let scene = synthetic_scene();
    let mesh = build_mesh(
        &scene,
        &ColorBlockCatalog::default(),
        &MeshLimits::default(),
    )
    .expect("mesh build");

    println!(
        "scene: {} blocks, {} quads, {} pages",
        scene.statistics.non_air, mesh.statistics.quads, mesh.statistics.pages
    );

    let available = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);
    println!("available parallelism: {available}");

    let mut thread_counts = if backend == RenderBackend::Cpu {
        vec![1, 2, available as u32]
    } else {
        vec![1]
    };
    thread_counts.sort_unstable();
    thread_counts.dedup();

    for threads in thread_counts {
        let options = RenderOptions {
            width,
            height,
            supersample,
            threads,
            backend,
            ..Default::default()
        };

        let mut samples = Vec::with_capacity(iterations as usize);
        for _ in 0..iterations {
            let started = Instant::now();
            render_mesh(&scene, &mesh, &options).expect("render");
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);

        let min = samples.first().copied().unwrap_or(0.0);
        let median = samples[samples.len() / 2];
        println!("threads {threads:>2}: min {min:8.2} ms, median {median:8.2} ms");
    }
}

fn parse_args(
    width: &mut u32,
    height: &mut u32,
    supersample: &mut u32,
    iterations: &mut u32,
    backend: &mut RenderBackend,
) {
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let Some(value) = args.next() else {
            break;
        };
        match flag.as_str() {
            "--width" => *width = value.parse().expect("width"),
            "--height" => *height = value.parse().expect("height"),
            "--supersample" => *supersample = value.parse().expect("supersample"),
            "--iterations" => *iterations = value.parse().expect("iterations"),
            "--backend" => *backend = RenderBackend::parse(&value).expect("backend"),
            _ => {}
        }
    }

    *iterations = (*iterations).max(1);
}

fn synthetic_scene() -> Scene {
    const SIZE_X: u32 = 48;
    const SIZE_Y: u32 = 32;
    const SIZE_Z: u32 = 48;

    let palette = vec![
        BlockStateKey::simple("minecraft:air"),
        BlockStateKey::simple("minecraft:stone"),
    ];
    let mut values = Vec::with_capacity((SIZE_X * SIZE_Y * SIZE_Z) as usize);

    for y in 0..SIZE_Y {
        for z in 0..SIZE_Z {
            for x in 0..SIZE_X {
                let height = 8 + (x * 7 + z * 13) % 20;
                let solid = y < height && (x + z + y) % 3 != 0;
                values.push(u32::from(solid));
            }
        }
    }

    let states =
        PackedIndices::from_values(PackedIndices::bits_for_palette(palette.len()), &values)
            .expect("packed indices");
    let region = Region {
        name: "bench".to_string(),
        origin: [0, 0, 0],
        size: [SIZE_X, SIZE_Y, SIZE_Z],
        data_version: 3700,
        palette,
        states,
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    };

    Scene::new(SceneMetadata::default(), vec![region], Vec::new()).expect("scene")
}
