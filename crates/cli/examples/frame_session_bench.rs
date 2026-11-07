//! Diagnostic: compare per-frame full GPU renders with one resident frame session.
//!
//! ```text
//! cargo run --release -p blockscope-cli --features gpu --example frame_session_bench -- \
//!   fixtures/private/000002.litematic
//! ```

#[cfg(feature = "gpu")]
fn main() -> Result<(), String> {
    use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
    use litematic_mesh::{ColorBlockCatalog, MeshLimits, build_mesh_parallel};
    use litematic_render::{GpuFrameSession, RenderBackend, RenderOptions, render_mesh};
    use std::hint::black_box;
    use std::time::Instant;

    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: frame_session_bench <file.litematic>".to_string())?;
    let input = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
    let decode_limits = DecodeLimits {
        max_input_bytes: 1 << 40,
        max_decompressed_bytes: 1 << 40,
        max_total_bytes: 1 << 40,
        ignore_block_entity_nbt: true,
        ..Default::default()
    };
    let scene_limits = LitematicLimits {
        max_volume_per_region: 1_000_000_000_000,
        max_total_volume: 1_000_000_000_000,
        ..Default::default()
    };
    let file = LitematicFile::parse_bytes(&input, &decode_limits, &scene_limits)
        .map_err(|error| error.to_string())?;
    let scene = file.into_scene().map_err(|error| error.to_string())?;

    let mesh_limits = MeshLimits {
        max_pages: usize::MAX,
        max_quads: usize::MAX,
        max_vertices: usize::MAX,
    };
    let catalog = ColorBlockCatalog::new().map_err(|error| error.to_string())?;
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = catalog.snapshot(states);
    let mesh = build_mesh_parallel(&scene, &snapshot, &mesh_limits, 12)
        .map_err(|error| error.to_string())?;
    eprintln!(
        "mesh: {} pages, {} quads",
        mesh.statistics.pages, mesh.statistics.quads
    );

    let base = RenderOptions {
        width: 256,
        height: 256,
        supersample: 1,
        threads: 12,
        sphere_fit: true,
        backend: RenderBackend::Gpu,
        ..Default::default()
    };
    let frames: Vec<RenderOptions> = (0..12)
        .map(|index| RenderOptions {
            yaw: 45.0 + index as f32 * 30.0,
            ..base.clone()
        })
        .collect();

    let per_frame_started = Instant::now();
    for frame in &frames {
        let output = render_mesh(&scene, &mesh, frame).map_err(|error| error.to_string())?;
        black_box(output);
    }
    let per_frame = per_frame_started.elapsed().as_secs_f64() * 1000.0;

    let session_started = Instant::now();
    let mut session =
        GpuFrameSession::new(&scene, &mesh, &frames[0], None).map_err(|error| error.to_string())?;
    session
        .render_frames(&frames, |_, output| {
            black_box(output);
            Ok(())
        })
        .map_err(|error| error.to_string())?;
    let resident = session.resident_bytes();
    let session_ms = session_started.elapsed().as_secs_f64() * 1000.0;

    println!(
        "frames={} per_frame_ms={per_frame:.1} session_ms={session_ms:.1} speedup={:.2}x resident_mb={:.1}",
        frames.len(),
        per_frame / session_ms,
        resident as f64 / (1024.0 * 1024.0)
    );
    Ok(())
}

#[cfg(not(feature = "gpu"))]
fn main() {
    eprintln!("build with --features gpu");
}
