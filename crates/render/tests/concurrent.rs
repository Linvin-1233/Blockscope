mod support;

use litematic_material::{AssetSource, MaterialSet, MaterialSnapshot};
use litematic_mesh::{Mesh, MeshLimits, build_mesh};
use litematic_render::{RenderOptions, render_mesh_with_textures, write_png};
use litematic_scene::Scene;
use std::sync::Arc;
use support::{pack, region, scene};

fn render_bytes(
    scene: &Scene,
    mesh: &Mesh,
    snapshot: &MaterialSnapshot,
    options: &RenderOptions,
) -> Vec<u8> {
    let output = render_mesh_with_textures(scene, mesh, options, snapshot.textures()).unwrap();
    let mut bytes = Vec::new();
    write_png(&output, &mut bytes).unwrap();
    bytes
}

#[test]
fn one_snapshot_serves_concurrent_renders() {
    let pack_dir = tempfile::tempdir().unwrap();
    pack(pack_dir.path());

    let mut source = AssetSource::new();
    source.add_dir(pack_dir.path()).unwrap();
    let materials = MaterialSet::open(source).unwrap();

    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 1],
    )]);
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = Arc::new(materials.snapshot(states));

    let mesh = build_mesh(&scene, snapshot.as_ref(), &MeshLimits::default()).unwrap();
    let options = RenderOptions {
        width: 96,
        height: 96,
        supersample: 2,
        ..Default::default()
    };
    let expected = render_bytes(&scene, &mesh, &snapshot, &options);

    let workers = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
        .min(8);
    let results: Vec<Vec<u8>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| scope.spawn(|| render_bytes(&scene, &mesh, &snapshot, &options)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("render worker panicked"))
            .collect()
    });

    for bytes in results {
        assert_eq!(bytes, expected, "concurrent render output must match");
    }
}
