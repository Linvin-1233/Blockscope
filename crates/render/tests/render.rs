mod support;

use litematic_mesh::{ColorBlockCatalog, Mesh, MeshLimits, Rgba, build_mesh};
use litematic_render::{
    Background, LightingProfile, RenderErrorCode, RenderOptions, RenderOutput, StreamingRenderer,
    render_mesh, write_png,
};
use litematic_scene::Scene;
use support::*;

fn build(scene: &Scene) -> Mesh {
    build_mesh(scene, &ColorBlockCatalog::default(), &MeshLimits::default()).unwrap()
}

fn render(scene: &Scene, options: &RenderOptions) -> RenderOutput {
    let mesh = build(scene);
    render_mesh(scene, &mesh, options).unwrap()
}

fn pixel(output: &RenderOutput, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * output.width + x) * 4) as usize;
    [
        output.rgba[index],
        output.rgba[index + 1],
        output.rgba[index + 2],
        output.rgba[index + 3],
    ]
}

fn non_transparent(output: &RenderOutput) -> u64 {
    output
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0)
        .count() as u64
}

fn alpha_bounds(output: &RenderOutput) -> (u32, u32, u32, u32) {
    let mut min_x = output.width;
    let mut min_y = output.height;
    let mut max_x = 0;
    let mut max_y = 0;

    for y in 0..output.height {
        for x in 0..output.width {
            if pixel(output, x, y)[3] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    (min_x, min_y, max_x, max_y)
}

fn stone_scene() -> Scene {
    scene(vec![region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    )])
}

fn options() -> RenderOptions {
    RenderOptions {
        width: 64,
        height: 64,
        ..Default::default()
    }
}

#[test]
fn renders_transparent_background_and_fitted_cube() {
    let output = render(&stone_scene(), &options());

    assert_eq!(output.rgba.len(), 64 * 64 * 4);
    assert_eq!(pixel(&output, 0, 0)[3], 0);
    assert_eq!(pixel(&output, 32, 32)[3], 255);

    let visible = non_transparent(&output);
    assert!(visible > 100);
    assert!(visible < 64 * 64);

    let (min_x, min_y, max_x, max_y) = alpha_bounds(&output);
    assert!(min_x >= 15 && min_y >= 15);
    assert!(max_x <= 48 && max_y <= 48);

    let center = pixel(&output, 32, 32);
    assert!(center[0] > 0);
    assert!(center[1] > 0);
    assert!(center[2] > 0);
}

#[test]
fn renders_solid_background() {
    let options = RenderOptions {
        background: Background::Solid(Rgba::rgb(10, 20, 30)),
        ..options()
    };
    let output = render(&stone_scene(), &options);

    assert_eq!(pixel(&output, 0, 0), [10, 20, 30, 255]);
    assert_eq!(pixel(&output, 32, 32)[3], 255);
}

#[test]
fn flat_and_ao_profiles_differ() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 2, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 0, 0, 1],
    )]);

    let flat = render(
        &scene,
        &RenderOptions {
            lighting: LightingProfile::FlatV1,
            ..options()
        },
    );
    let ao = render(
        &scene,
        &RenderOptions {
            lighting: LightingProfile::SimpleAoV1,
            ..options()
        },
    );

    assert_ne!(flat.rgba, ao.rgba);
}

#[test]
fn translucent_water_keeps_partial_alpha() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:water"],
        &[1],
    )]);
    let output = render(&scene, &options());

    let partial = output
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] > 0 && pixel[3] < 255)
        .count();
    assert!(partial > 0);
}

#[test]
fn png_output_is_deterministic() {
    let output = render(&stone_scene(), &options());

    let mut first = Vec::new();
    write_png(&output, &mut first).unwrap();
    let mut second = Vec::new();
    write_png(&output, &mut second).unwrap();

    assert!(first.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]));
    assert_eq!(first, second);
}

#[test]
fn parallel_tiles_match_single_thread() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 2, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 0, 0, 1],
    )]);

    let single = render(
        &scene,
        &RenderOptions {
            width: 96,
            height: 96,
            threads: 1,
            ..Default::default()
        },
    );
    let parallel = render(
        &scene,
        &RenderOptions {
            width: 96,
            height: 96,
            threads: 4,
            ..Default::default()
        },
    );

    assert_eq!(single.rgba, parallel.rgba);
    assert_eq!(single.statistics, parallel.statistics);
}

#[test]
fn tile_size_does_not_change_output() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [2, 2, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 0, 0, 1],
    )]);
    let render_with = |tile_size| {
        render(
            &scene,
            &RenderOptions {
                width: 256,
                height: 256,
                supersample: 1,
                tile_size,
                threads: 4,
                ..Default::default()
            },
        )
    };

    let large = render_with(64);
    let small = render_with(8);

    assert_eq!(large.rgba, small.rgba);
    assert_eq!(
        large.statistics.pixels_tested,
        small.statistics.pixels_tested
    );
    assert_eq!(
        large.statistics.pixels_written,
        small.statistics.pixels_written
    );
    assert!(large.statistics.tiles < small.statistics.tiles);
}

#[test]
fn multi_page_parallel_render_matches_single_thread() {
    let mut values = vec![0u32; 34 * 34];
    for (i, v) in values.iter_mut().enumerate() {
        if i % 3 == 0 {
            *v = 1;
        }
    }
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [34, 34, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    )]);

    let single = render(
        &scene,
        &RenderOptions {
            width: 128,
            height: 128,
            threads: 1,
            ..Default::default()
        },
    );
    for threads in [2, 4, 8] {
        let parallel = render(
            &scene,
            &RenderOptions {
                width: 128,
                height: 128,
                threads,
                ..Default::default()
            },
        );
        assert_eq!(single.rgba, parallel.rgba, "threads={threads}");
        assert_eq!(single.statistics, parallel.statistics, "threads={threads}");
    }
}

#[test]
fn supersampling_changes_edge_pixels() {
    let scene = stone_scene();
    let single = render(
        &scene,
        &RenderOptions {
            supersample: 1,
            ..options()
        },
    );
    let supersampled = render(
        &scene,
        &RenderOptions {
            supersample: 2,
            ..options()
        },
    );

    assert_eq!(single.rgba.len(), supersampled.rgba.len());
    assert_ne!(single.rgba, supersampled.rgba);
}

#[test]
fn rejects_supersampled_size_limit() {
    let scene = stone_scene();
    let options = RenderOptions {
        width: 2048,
        height: 2048,
        supersample: 4,
        ..Default::default()
    };

    let error = render_mesh(&scene, &build(&scene), &options).unwrap_err();
    assert_eq!(error.code, RenderErrorCode::OutputTooLarge);
}

#[test]
fn empty_mesh_renders_background_only() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air"],
        &[0],
    )]);

    let transparent = render(&scene, &options());
    assert!(transparent.rgba.iter().all(|value| *value == 0));

    let solid = render(
        &scene,
        &RenderOptions {
            background: Background::Solid(Rgba::rgb(1, 2, 3)),
            ..options()
        },
    );
    assert_eq!(pixel(&solid, 0, 0), [1, 2, 3, 255]);
}

#[test]
fn streaming_matches_the_table_path() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [3, 2, 1],
        &["minecraft:air", "minecraft:stone", "minecraft:water"],
        &[1, 2, 1, 0, 2, 1],
    )]);
    let mesh = build_mesh(
        &scene,
        &ColorBlockCatalog::default(),
        &MeshLimits::default(),
    )
    .unwrap();
    let base = RenderOptions {
        width: 96,
        height: 96,
        supersample: 2,
        ..Default::default()
    };

    let table = render_mesh(&scene, &mesh, &base).unwrap();
    let streaming = render_mesh(
        &scene,
        &mesh,
        &RenderOptions {
            force_streaming: true,
            ..base
        },
    )
    .unwrap();

    assert_eq!(table.rgba, streaming.rgba);
    assert_eq!(table.statistics, streaming.statistics);
}

#[test]
fn streaming_renderer_matches_the_table_path_page_by_page() {
    let scene = scene(vec![region(
        "main",
        [0, 0, 0],
        [3, 2, 1],
        &["minecraft:air", "minecraft:stone", "minecraft:water"],
        &[1, 2, 1, 0, 2, 1],
    )]);
    let mesh = build_mesh(
        &scene,
        &ColorBlockCatalog::default(),
        &MeshLimits::default(),
    )
    .unwrap();
    let options = RenderOptions {
        width: 96,
        height: 96,
        supersample: 2,
        ..Default::default()
    };

    let table = render_mesh(&scene, &mesh, &options).unwrap();
    let mut renderer = StreamingRenderer::new(&scene, &options, None).unwrap();
    for page in &mesh.pages {
        renderer.render_pages(std::slice::from_ref(page));
    }
    let streamed = renderer.finish();

    assert_eq!(table.rgba, streamed.rgba);
    assert_eq!(table.statistics, streamed.statistics);
}

#[test]
fn rejects_invalid_options() {
    let scene = stone_scene();

    let too_large = RenderOptions {
        width: 5000,
        height: 5000,
        ..options()
    };
    let error = render_mesh(&scene, &build(&scene), &too_large).unwrap_err();
    assert_eq!(error.code, RenderErrorCode::OutputTooLarge);

    let bad_elevation = RenderOptions {
        elevation: 90.0,
        ..options()
    };
    let error = render_mesh(&scene, &build(&scene), &bad_elevation).unwrap_err();
    assert_eq!(error.code, RenderErrorCode::InvalidOptions);
}
