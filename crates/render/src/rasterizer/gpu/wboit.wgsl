@group(0) @binding(0) var accum: texture_2d<f32>;
@group(0) @binding(1) var reveal: texture_2d<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let dimensions = textureDimensions(accum);
    let max_coord = vec2<i32>(dimensions) - vec2<i32>(1);
    let coords = clamp(vec2<i32>(input.position.xy), vec2<i32>(0), max_coord);
    let accumulated = textureLoad(accum, coords, 0);
    let revealage = textureLoad(reveal, coords, 0).a;
    let alpha = 1.0 - revealage;
    if (alpha <= 0.0) {
        discard;
    }
    let color = accumulated.rgb / max(accumulated.a, 1e-5);
    return vec4<f32>(color * alpha, alpha);
}
