use litematic_mesh::{AO_LEVELS, Quad, Vertex, ambient_occlusion_level, decode_uv, encode_uv};
use std::mem::size_of;

#[test]
fn q12_uv_values_round_trip_exactly() {
    let values = [
        -1.28125f32,
        -0.28125,
        -0.0625,
        0.0,
        1.0 / 16.0,
        0.21875,
        0.5,
        1.0,
        1.28125,
    ];

    for value in values {
        let encoded = encode_uv([value, -value]);
        assert_eq!(decode_uv(encoded), [value, -value], "value {value}");
    }
}

#[test]
fn q12_uv_encoding_clamps_out_of_range_values() {
    assert_eq!(encode_uv([100.0, -100.0]), [i16::MAX, i16::MIN]);
}

#[test]
fn ambient_occlusion_levels_match_the_old_float_values() {
    assert_eq!(ambient_occlusion_level(0), 0.55f32);
    assert_eq!(
        ambient_occlusion_level(1),
        0.55f32 + 0.45f32 * (1.0f32 - 2.0f32 / 3.0f32)
    );
    assert_eq!(
        ambient_occlusion_level(2),
        0.55f32 + 0.45f32 * (1.0f32 - 1.0f32 / 3.0f32)
    );
    assert_eq!(ambient_occlusion_level(3), 1.0f32);
    assert_eq!(AO_LEVELS, [0.55, 0.7, 0.85, 1.0]);
}

#[test]
fn compact_vertices_stay_small() {
    // Guards the memory budget: these budgets leave room for alignment but
    // catch a regression back to f32 UVs or f32 ambient occlusion.
    assert!(size_of::<Vertex>() <= 16, "{}", size_of::<Vertex>());
    assert!(size_of::<Quad>() <= 88, "{}", size_of::<Quad>());
}
