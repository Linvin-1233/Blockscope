// Instanced opaque/cutout path. One instance is one quad; six vertices select
// the two triangles and the shader performs the camera transform.
//
// Texture sampling mirrors shader.wgsl so atlas edges, wrapping, and filtering
// stay identical between the old and new geometry paths.

struct Globals {
    filter_mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

struct Camera {
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    center: vec4<f32>,
    size: vec4<f32>,
};

@group(0) @binding(0) var atlas: texture_2d<f32>;
@group(0) @binding(1) var<uniform> globals: Globals;
@group(0) @binding(2) var<storage, read> rects: array<vec4<u32>>;
@group(1) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) p0: vec3<f32>,
    @location(1) p1: vec3<f32>,
    @location(2) p2: vec3<f32>,
    @location(3) p3: vec3<f32>,
    @location(4) uv0: vec2<i32>,
    @location(5) uv1: vec2<i32>,
    @location(6) uv2: vec2<i32>,
    @location(7) uv3: vec2<i32>,
    @location(8) page_offset: vec3<f32>,
    @location(9) color: vec4<f32>,
    @location(10) ao: vec4<u32>,
    @location(11) shade: f32,
    @location(12) texture: u32,
    @location(13) flags: u32,
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

fn corner_index(vertex_index: u32) -> u32 {
    if (vertex_index == 1u) {
        return 1u;
    }
    if (vertex_index == 2u) {
        return 2u;
    }
    if (vertex_index == 3u) {
        return 0u;
    }
    if (vertex_index == 4u) {
        return 2u;
    }
    if (vertex_index == 5u) {
        return 3u;
    }
    return 0u;
}

fn ao_factor(level: u32) -> f32 {
    if (level == 0u) {
        return 0.55;
    }
    if (level == 1u) {
        return 0.55 + 0.45 * (1.0 - 2.0 / 3.0);
    }
    if (level == 2u) {
        return 0.55 + 0.45 * (1.0 - 1.0 / 3.0);
    }
    return 1.0;
}

@vertex
fn vs_main(input: VertexInput, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var positions = array<vec3<f32>, 4>(input.p0, input.p1, input.p2, input.p3);
    var uvs = array<vec2<i32>, 4>(input.uv0, input.uv1, input.uv2, input.uv3);
    var ao_levels = array<u32, 4>(input.ao.x, input.ao.y, input.ao.z, input.ao.w);
    let corner = corner_index(vertex_index);
    let local = positions[corner];
    let relative = input.page_offset + local - camera.center.xyz;
    let x = dot(relative, camera.right.xyz);
    let y = dot(relative, camera.up.xyz);
    let depth = dot(relative, camera.forward.xyz);
    let half_width = camera.size.x * 0.5;
    let half_height = camera.size.y * 0.5;
    let clip_x = x * camera.right.w / half_width;
    // CPU screen y is height/2 - y*scale; wgpu clip +Y points up, so the
    // double flip cancels and this term is positive.
    let clip_y = y * camera.right.w / half_height;
    let clip_z = (depth - camera.size.z) / camera.size.w;
    let ao = ao_factor(ao_levels[corner]);

    // Reject per-view backfaces. Single-view rendering already culled on the
    // CPU, but multi-view shares one upload and needs the shader to cull for
    // each camera.
    let code = (input.flags >> 2u) & 0x3fu;
    let normal = vec3<f32>(
        f32(code & 3u) - 1.0,
        f32((code >> 2u) & 3u) - 1.0,
        f32((code >> 4u) & 3u) - 1.0,
    );
    let facing = dot(normal, camera.forward.xyz);

    var out: VertexOutput;
    out.clip_position = vec4<f32>(clip_x, clip_y, clamp(clip_z, 0.0, 0.999999), 1.0);
    if (facing >= 0.0) {
        out.clip_position = vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    out.color = vec4<f32>(input.color.rgb * input.shade * ao, input.color.a);
    out.uv = vec2<f32>(uvs[corner]) / UV_SCALE;
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
    return vec4<f32>(color.rgb * color.a, color.a);
}
