//! Diagnostic: textured resident frame session for preview latency.
//!
//! ```text
//! cargo run --release -p blockscope-cli --features gpu \
//!   --example frame_session_texture_bench -- \
//!   fixtures/private/000002.litematic fixtures/private/texture/26.2-Fabric.jar 128 128
//! ```

#[cfg(feature = "gpu")]
fn main() -> Result<(), String> {
    use blockscope_core::{DecodeLimits, LitematicFile, LitematicLimits};
    use blockscope_material::{AssetSource, MaterialSet};
    use blockscope_mesh::{MeshLimits, build_mesh_parallel};
    use blockscope_render::{GpuFrameSession, RenderBackend, RenderOptions, write_png};
    use std::time::Instant;

    const USAGE: &str = "usage: frame_session_texture_bench <file> <jar> <width> <height>";

    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or_else(|| USAGE.to_string())?;
    let jar = args.next().ok_or_else(|| USAGE.to_string())?;
    let width: u32 = args
        .next()
        .ok_or_else(|| "missing width".to_string())?
        .parse()
        .map_err(|error| format!("width: {error}"))?;
    let height: u32 = args
        .next()
        .ok_or_else(|| "missing height".to_string())?
        .parse()
        .map_err(|error| format!("height: {error}"))?;

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

    let mut source = AssetSource::new();
    source
        .add_jar(std::path::Path::new(&jar))
        .map_err(|error| error.to_string())?;
    let materials = MaterialSet::open(source).map_err(|error| error.to_string())?;
    let states = scene
        .regions
        .iter()
        .flat_map(|region| region.palette.iter().cloned());
    let snapshot = materials.snapshot(states);
    let texture_bytes = snapshot.textures().bytes();

    let mesh_limits = MeshLimits {
        max_pages: usize::MAX,
        max_quads: usize::MAX,
        max_vertices: usize::MAX,
    };
    let mesh_started = Instant::now();
    let mesh = build_mesh_parallel(&scene, &snapshot, &mesh_limits, 12)
        .map_err(|error| error.to_string())?;
    let mesh_ms = mesh_started.elapsed().as_secs_f64() * 1000.0;
    eprintln!(
        "mesh: {} pages, {} quads, {:.1} ms; textures: {} bytes",
        mesh.statistics.pages, mesh.statistics.quads, mesh_ms, texture_bytes
    );

    let base = RenderOptions {
        width,
        height,
        supersample: 1,
        threads: 12,
        sphere_fit: true,
        backend: RenderBackend::Gpu,
        ..Default::default()
    };
    let frames: Vec<RenderOptions> = (0..60)
        .map(|index| RenderOptions {
            yaw: 45.0 + index as f32 * 6.0,
            ..base.clone()
        })
        .collect();

    let setup_started = Instant::now();
    let mut session =
        match GpuFrameSession::new(&scene, &mesh, &frames[0], Some(snapshot.textures())) {
            Ok(session) => session,
            Err(error) => {
                println!(
                    "{}",
                    serde_json::json!({
                        "width": width,
                        "height": height,
                        "ok": false,
                        "error": error.to_string(),
                    })
                );
                return Err(error.to_string());
            }
        };
    let setup_ms = setup_started.elapsed().as_secs_f64() * 1000.0;
    let resident_mb = session.resident_bytes() as f64 / (1024.0 * 1024.0);

    for _ in 0..2 {
        session
            .render_frame(&frames[0])
            .map_err(|error| error.to_string())?;
    }

    let mut render_ms = Vec::with_capacity(frames.len());
    let mut total_ms = Vec::with_capacity(frames.len());
    for frame in &frames {
        let started = Instant::now();
        let output = session
            .render_frame(frame)
            .map_err(|error| error.to_string())?;
        let render = started.elapsed().as_secs_f64() * 1000.0;
        let mut png = Vec::new();
        write_png(&output, &mut png).map_err(|error| error.to_string())?;
        let total = started.elapsed().as_secs_f64() * 1000.0;
        render_ms.push(render);
        total_ms.push(total);
    }

    render_ms.sort_by(|left, right| left.total_cmp(right));
    total_ms.sort_by(|left, right| left.total_cmp(right));
    let percentile = |values: &[f64], p: f64| {
        let index = ((values.len() as f64 * p).ceil() as usize).saturating_sub(1);
        values[index.min(values.len() - 1)]
    };
    println!(
        "{}",
        serde_json::json!({
            "width": width,
            "height": height,
            "ok": true,
            "frames": frames.len(),
            "meshMs": mesh_ms,
            "setupMs": setup_ms,
            "residentMb": resident_mb,
            "textureBytes": texture_bytes,
            "renderP50Ms": percentile(&render_ms, 0.5),
            "renderP95Ms": percentile(&render_ms, 0.95),
            "totalP50Ms": percentile(&total_ms, 0.5),
            "totalP95Ms": percentile(&total_ms, 0.95),
        })
    );
    Ok(())
}

#[cfg(not(feature = "gpu"))]
fn main() {
    eprintln!("build with --features gpu");
}
