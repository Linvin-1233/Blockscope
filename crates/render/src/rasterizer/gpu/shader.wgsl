// Offscreen rasterizer for projected color-block quads.
//
// The CPU rasterizer samples textures in texel space with manual wrapping.
// Keeping that behavior here (instead of using a sampler) makes nearest and
// bilinear filtering independent of sampler state and keeps atlas edges from
// bleeding into neighboring textures.

struct Globals {
    filter_mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<storage, read> rects: array<vec4<u32>>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<i32>,
    @location(3) texture: u32,
    @location(4) flags: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) texture: u32,
    @location(3) @interpolate(flat) flags: u32,
};

const UV_SCALE: f32 = 4096.0;
const FLAG_CUTOUT: u32 = 1u;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(input.position, 1.0);
    out.color = input.color;
    out.uv = vec2<f32>(input.uv) / UV_SCALE;
    out.texture = input.texture;
    out.flags = input.flags;
    return out;
}

fn repeat01(value: f32) -> f32 {
    return value - floor(value);
}

fn wrap(value: i32, size: i32) -> i32 {
    return ((value % size) + size) % size;
}

fn load(origin: vec2<i32>, size: vec2<i32>, texel: vec2<i32>) -> vec4<f32> {
    let wrapped = vec2<i32>(wrap(texel.x, size.x), wrap(texel.y, size.y));
    return textureLoad(atlas, origin + wrapped, 0);
}

fn sample_texture(id: u32, uv: vec2<f32>) -> vec4<f32> {
    if (id >= arrayLength(&rects)) {
        return vec4<f32>(1.0);
    }
    let rect = rects[id];
    let size = vec2<i32>(i32(rect.z - rect.x), i32(rect.w - rect.y));
    if (size.x == 0 || size.y == 0) {
        return vec4<f32>(1.0);
    }

    let origin = vec2<i32>(rect.xy);
    let position = vec2<f32>(repeat01(uv.x), repeat01(uv.y)) * vec2<f32>(size);
    if (globals.filter_mode == 0u) {
        let texel = vec2<i32>(i32(floor(position.x)), i32(floor(position.y)));
        return load(origin, size, texel);
    }

    // Match the CPU bilinear path: shift by half a texel, interpolate the four
    // wrapped neighbors.
    let shifted = position - vec2<f32>(0.5);
    let base = vec2<i32>(i32(floor(shifted.x)), i32(floor(shifted.y)));
    let fraction = shifted - floor(shifted);
    let c00 = load(origin, size, base);
    let c10 = load(origin, size, base + vec2<i32>(1, 0));
    let c01 = load(origin, size, base + vec2<i32>(0, 1));
    let c11 = load(origin, size, base + vec2<i32>(1, 1));
    let top = mix(c00, c10, fraction.x);
    let bottom = mix(c01, c11, fraction.x);
    return mix(top, bottom, fraction.y);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var color = input.color;
    if (input.texture < arrayLength(&rects)) {
        color *= sample_texture(input.texture, input.uv);
    }
    if ((input.flags & FLAG_CUTOUT) != 0u && color.a < 0.5) {
        discard;
    }
    // Premultiplied output lets the translucent pipeline use source-over
    // blending with One / OneMinusSrcAlpha, matching the CPU accumulator.
    return vec4<f32>(color.rgb * color.a, color.a);
}
