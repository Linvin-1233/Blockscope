mod support;

use litematic_mesh::{
    ColorBlockCatalog, ColorSnapshot, MeshBuildOptions, MeshLimits,
    build_mesh_parallel_with_options_and_progress, build_mesh_streaming_with_options_and_progress,
};
use std::sync::atomic::{AtomicU64, Ordering};
use support::{region as make_region, scene as make_scene};

fn page_scene() -> litematic_scene::Scene {
    let values = vec![1u32; 16 * 16];
    let region = make_region(
        "main",
        [0, 0, 0],
        [16, 1, 16],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    make_scene(vec![region])
}

fn snapshot(scene: &litematic_scene::Scene) -> (ColorBlockCatalog, ColorSnapshot) {
    let catalog = ColorBlockCatalog::default();
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = catalog.snapshot(states);
    (catalog, snapshot)
}

#[test]
fn parallel_builder_reports_page_progress() {
    let scene = page_scene();
    let (_catalog, snapshot) = snapshot(&scene);
    let limits = MeshLimits::default();
    let calls = AtomicU64::new(0);
    let highest = AtomicU64::new(0);
    let total_seen = AtomicU64::new(0);

    let mesh = build_mesh_parallel_with_options_and_progress(
        &scene,
        &snapshot,
        &limits,
        4,
        MeshBuildOptions::default(),
        Some(&|done, total| {
            calls.fetch_add(1, Ordering::Relaxed);
            highest.fetch_max(done, Ordering::Relaxed);
            total_seen.store(total, Ordering::Relaxed);
        }),
    )
    .unwrap();

    assert_eq!(mesh.statistics.pages, 1);
    assert_eq!(total_seen.load(Ordering::Relaxed), 1);
    assert_eq!(highest.load(Ordering::Relaxed), 1);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn streaming_builder_reports_page_progress() {
    let scene = page_scene();
    let (_catalog, snapshot) = snapshot(&scene);
    let limits = MeshLimits::default();
    let calls = AtomicU64::new(0);
    let highest = AtomicU64::new(0);
    let total_seen = AtomicU64::new(0);
    let mut pages = 0;

    let statistics = build_mesh_streaming_with_options_and_progress(
        &scene,
        &snapshot,
        &limits,
        4,
        MeshBuildOptions::default(),
        Some(&|done, total| {
            calls.fetch_add(1, Ordering::Relaxed);
            highest.fetch_max(done, Ordering::Relaxed);
            total_seen.store(total, Ordering::Relaxed);
        }),
        |chunk| pages += chunk.len(),
    )
    .unwrap();

    assert_eq!(statistics.pages, 1);
    assert_eq!(pages, 1);
    assert_eq!(total_seen.load(Ordering::Relaxed), 1);
    assert_eq!(highest.load(Ordering::Relaxed), 1);
    assert!(calls.load(Ordering::Relaxed) >= 2);
}
