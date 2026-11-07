//! Compare exact and fast CPU projection on one parsed scene and mesh.
//!
//! Usage: `cargo run --release -p blockscope-cli --example precision_bench -- \
//!   fixtures/private/000001.litematic fixtures/private/texture/26.2-Fabric.jar`

use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use litematic_material::{AssetSource, MaterialSet};
use litematic_mesh::{MeshBuildOptions, MeshLimits, build_mesh_parallel_with_options};
use litematic_render::{RenderBackend, RenderOptions, RenderPrecision, render_mesh_with_textures};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let fixture = PathBuf::from(args.next().expect("fixture path"));
    let jar = PathBuf::from(args.next().expect("client jar path"));
    let iterations: usize = args
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(6);

    let bytes = std::fs::read(&fixture).expect("read fixture");
    let file = LitematicFile::parse_bytes(
        &bytes,
        &DecodeLimits::default(),
        &LitematicLimits::default(),
    )
    .expect("parse litematic");
    let scene = file.into_scene().expect("build scene");

    let mut source = AssetSource::new();
    source.add_jar(&jar).expect("open client jar");
    let materials = MaterialSet::open(source).expect("open materials");
    let snapshot = materials.snapshot(
        scene
            .regions
            .iter()
            .flat_map(|region| region.palette.iter().cloned()),
    );

    let limits = MeshLimits {
        max_pages: 65_536,
        max_quads: 200_000_000,
        max_vertices: 800_000_000,
    };
    let mesh = build_mesh_parallel_with_options(
        &scene,
        &snapshot,
        &limits,
        12,
        MeshBuildOptions::default(),
    )
    .expect("build mesh");
    println!("quads: {}", mesh.statistics.quads);

    let base = RenderOptions {
        width: 1024,
        height: 1024,
        supersample: 2,
        threads: 12,
        backend: RenderBackend::Cpu,
        ..Default::default()
    };
    let exact = RenderOptions {
        precision: RenderPrecision::Exact,
        ..base.clone()
    };
    let fast = RenderOptions {
        precision: RenderPrecision::Fast,
        ..base
    };

    let mut exact_times = Vec::new();
    let mut fast_times = Vec::new();
    let mut last_exact = None;
    let mut last_fast = None;
    for index in 0..iterations {
        let precision = if index % 2 == 0 { "exact" } else { "fast" };
        let started = Instant::now();
        if precision == "exact" {
            let output = render_mesh_with_textures(&scene, &mesh, &exact, snapshot.textures())
                .expect("render");
            last_exact = Some(output.clone());
        } else {
            let output = render_mesh_with_textures(&scene, &mesh, &fast, snapshot.textures())
                .expect("render");
            last_fast = Some(output.clone());
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!("{precision}: {elapsed:.1} ms");
        if precision == "exact" {
            exact_times.push(elapsed);
        } else {
            fast_times.push(elapsed);
        }
    }

    report("exact", &mut exact_times);
    report("fast", &mut fast_times);

    if let (Some(exact), Some(fast)) = (last_exact, last_fast) {
        let mut differing = 0u64;
        let mut alpha_mismatch = 0u64;
        let mut max_delta = 0u8;
        let mut buckets = [0u64; 6];
        for (left, right) in exact
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(fast.rgba.as_chunks::<4>().0)
        {
            if left != right {
                differing += 1;
                if left[3] != right[3] {
                    alpha_mismatch += 1;
                }
                let mut pixel_max = 0u8;
                for channel in 0..4 {
                    pixel_max = pixel_max.max(left[channel].abs_diff(right[channel]));
                }
                max_delta = max_delta.max(pixel_max);
                let bucket = match pixel_max {
                    0 => 0,
                    1 => 1,
                    2..=4 => 2,
                    5..=16 => 3,
                    17..=64 => 4,
                    _ => 5,
                };
                buckets[bucket] += 1;
            }
        }
        let pixels = exact.rgba.len() as u64 / 4;
        println!(
            "diff: {differing}/{pixels} pixels ({:.3}%), alpha mismatch {alpha_mismatch}, max delta {max_delta}",
            differing as f64 / pixels as f64 * 100.0
        );
        println!(
            "delta buckets: 1={} 2-4={} 5-16={} 17-64={} 65+={}",
            buckets[1], buckets[2], buckets[3], buckets[4], buckets[5]
        );
    }
}

fn report(name: &str, samples: &mut [f64]) {
    if samples.is_empty() {
        return;
    }
    samples.sort_by(f64::total_cmp);
    let median = samples[samples.len() / 2];
    let min = samples[0];
    let max = samples[samples.len() - 1];
    println!("{name}: min {min:.1} ms, median {median:.1} ms, max {max:.1} ms");
}
