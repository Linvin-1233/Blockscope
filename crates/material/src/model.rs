use crate::MaterialError;
use crate::json::{Element, ElementRotation, Face, ModelFile, TextureRef};
use crate::source::AssetSource;
use crate::texture::TextureSet;
use crate::tint::tint_for;
use litematic_mesh::{BlockModel, FaceDirection, ModelQuad, NO_TEXTURE, RenderLayer, face_uv};
use litematic_scene::BlockStateKey;
use std::collections::{BTreeMap, HashMap};

const MAX_PARENT_DEPTH: usize = 64;

/// A model after parent inheritance, before geometry is built.
#[derive(Debug, Clone, Default)]
pub struct ModelChain {
    pub textures: BTreeMap<String, TextureRef>,
    pub elements: Option<Vec<Element>>,
    pub ambient_occlusion: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedTexture {
    pub path: String,
    pub force_translucent: bool,
}

/// Load one model and resolve its parent chain. Missing or invalid models
/// return `None` so the catalog can fall back to built-in geometry.
pub fn resolve_model(
    source: &AssetSource,
    path: &str,
    cache: &mut HashMap<String, Option<ModelChain>>,
    stack: &mut Vec<String>,
) -> Option<ModelChain> {
    let path = normalize_model_path(path);
    if let Some(cached) = cache.get(&path) {
        return cached.clone();
    }
    if stack.iter().any(|item| item == &path) || stack.len() >= MAX_PARENT_DEPTH {
        return None;
    }

    let file = match load_model_file(source, &path) {
        Ok(Some(file)) => file,
        Ok(None) => {
            cache.insert(path, None);
            return None;
        }
        Err(_) => {
            cache.insert(path, None);
            return None;
        }
    };

    stack.push(path.clone());
    let mut chain = if let Some(parent) = &file.parent {
        resolve_model(source, parent, cache, stack).unwrap_or_else(|| ModelChain {
            ambient_occlusion: true,
            ..Default::default()
        })
    } else {
        ModelChain {
            ambient_occlusion: true,
            ..Default::default()
        }
    };
    stack.pop();

    for (name, texture) in file.textures {
        chain.textures.insert(name, texture);
    }
    if file.elements.is_some() {
        chain.elements = file.elements;
    }
    if let Some(ambient_occlusion) = file.ambientocclusion {
        chain.ambient_occlusion = ambient_occlusion;
    }

    cache.insert(path, Some(chain.clone()));
    Some(chain)
}

fn load_model_file(source: &AssetSource, path: &str) -> Result<Option<ModelFile>, MaterialError> {
    let asset = format!("assets/minecraft/models/{path}.json");
    let Some(bytes) = source.read(&asset)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| MaterialError::json(format!("{asset}: {error}")))
}

fn normalize_model_path(path: &str) -> String {
    let path = path.strip_prefix("minecraft:").unwrap_or(path);
    path.trim_start_matches("models/").to_string()
}

/// Build textured geometry for one selected blockstate variant.
#[allow(clippy::too_many_arguments)]
pub fn build_model(
    chain: &ModelChain,
    state: &BlockStateKey,
    x_rotation: i32,
    y_rotation: i32,
    uvlock: bool,
    source: &AssetSource,
    textures: &mut TextureSet,
) -> Option<BlockModel> {
    let elements = chain.elements.as_ref()?;
    if elements.is_empty() {
        return None;
    }

    let mut quads = Vec::new();
    for element in elements {
        for (face_name, face) in &element.faces {
            let Some(direction) = face_direction(face_name) else {
                continue;
            };
            let Some(texture) = resolve_texture(&face.texture, &chain.textures) else {
                continue;
            };
            let texture_id = textures.load(source, &texture.path).unwrap_or(NO_TEXTURE);
            let layer = texture
                .force_translucent
                .then_some(RenderLayer::Translucent);

            let base_vertices = face_vertices(direction, element.from, element.to);
            let mut uv = face_uv_for_json(direction, &base_vertices, face);

            let mut vertices = base_vertices;
            let mut cullface = face.cullface.as_deref().and_then(face_direction);

            if let Some(rotation) = &element.rotation {
                vertices = vertices.map(|vertex| rotate_element(vertex, rotation));
                cullface = None;
            }

            // Minecraft's blockstate x/y rotations use the opposite sign from
            // the right-handed rotation used by the mesh helpers.
            vertices = vertices.map(|vertex| rotate_block(vertex, -x_rotation, -y_rotation));
            cullface = match cullface {
                Some(face) => rotate_face(face, -x_rotation, -y_rotation),
                None => None,
            };

            if uvlock {
                rotate_uvs(&mut uv, y_rotation);
                rotate_uvs(&mut uv, x_rotation);
            }

            let direction = rotate_face(direction, -x_rotation, -y_rotation).unwrap_or(direction);
            let normal = geometric_normal(vertices)?;

            quads.push(ModelQuad {
                vertices,
                normal,
                face: direction,
                cullface,
                uv,
                texture: texture_id,
                tint: tint_for(state, face.tintindex),
                layer,
                shade: element.shade.unwrap_or(true),
            });
        }
    }

    if quads.is_empty() {
        return None;
    }

    Some(BlockModel {
        quads,
        is_full_cube: is_full_cube(elements),
        ambient_occlusion: chain.ambient_occlusion,
    })
}

fn is_full_cube(elements: &[Element]) -> bool {
    elements.iter().any(|element| {
        element.from == [0.0, 0.0, 0.0]
            && element.to == [16.0, 16.0, 16.0]
            && element.faces.len() == 6
    })
}

fn face_direction(name: &str) -> Option<FaceDirection> {
    match name {
        "down" => Some(FaceDirection::Down),
        "up" => Some(FaceDirection::Up),
        "north" => Some(FaceDirection::North),
        "south" => Some(FaceDirection::South),
        "west" => Some(FaceDirection::West),
        "east" => Some(FaceDirection::East),
        _ => None,
    }
}

fn face_vertices(face: FaceDirection, from: [f32; 3], to: [f32; 3]) -> [[f32; 3]; 4] {
    let [x1, y1, z1] = from;
    let [x2, y2, z2] = to;

    match face {
        FaceDirection::Down => [[x1, y1, z2], [x1, y1, z1], [x2, y1, z1], [x2, y1, z2]],
        FaceDirection::Up => [[x1, y2, z1], [x1, y2, z2], [x2, y2, z2], [x2, y2, z1]],
        FaceDirection::North => [[x1, y1, z1], [x1, y2, z1], [x2, y2, z1], [x2, y1, z1]],
        FaceDirection::South => [[x2, y1, z2], [x2, y2, z2], [x1, y2, z2], [x1, y1, z2]],
        FaceDirection::West => [[x1, y1, z2], [x1, y2, z2], [x1, y2, z1], [x1, y1, z1]],
        FaceDirection::East => [[x2, y1, z1], [x2, y2, z1], [x2, y2, z2], [x2, y1, z2]],
    }
}

fn face_uv_for_json(face: FaceDirection, vertices: &[[f32; 3]; 4], json: &Face) -> [[f32; 2]; 4] {
    let mut uv = match json.uv {
        Some(rect) => map_uv_rect(face, vertices, rect, 16.0),
        None => face_uv(face, *vertices),
    };

    if let Some(rotation) = json.rotation {
        rotate_uvs(&mut uv, rotation);
    }
    uv
}

fn map_uv_rect(
    face: FaceDirection,
    vertices: &[[f32; 3]; 4],
    rect: [f32; 4],
    divisor: f32,
) -> [[f32; 2]; 4] {
    let mut points = [[0.0f32; 2]; 4];
    let mut min_a = f32::INFINITY;
    let mut max_a = f32::NEG_INFINITY;
    let mut min_b = f32::INFINITY;
    let mut max_b = f32::NEG_INFINITY;

    for (index, vertex) in vertices.iter().enumerate() {
        let (a, b) = face_axes(face, *vertex);
        points[index] = [a, b];
        min_a = min_a.min(a);
        max_a = max_a.max(a);
        min_b = min_b.min(b);
        max_b = max_b.max(b);
    }

    let [u1, v1, u2, v2] = rect;
    let span_a = (max_a - min_a).max(1e-6);
    let span_b = (max_b - min_b).max(1e-6);

    let mut uv = [[0.0f32; 2]; 4];
    for (index, [a, b]) in points.into_iter().enumerate() {
        let u = u1 + (a - min_a) / span_a * (u2 - u1);
        let v = v1 + (b - min_b) / span_b * (v2 - v1);
        uv[index] = [u / divisor, v / divisor];
    }
    uv
}

fn face_axes(face: FaceDirection, vertex: [f32; 3]) -> (f32, f32) {
    match face {
        FaceDirection::Down => (vertex[0], 16.0 - vertex[2]),
        FaceDirection::Up => (vertex[0], vertex[2]),
        FaceDirection::North => (16.0 - vertex[0], 16.0 - vertex[1]),
        FaceDirection::South => (vertex[0], 16.0 - vertex[1]),
        FaceDirection::West => (vertex[2], 16.0 - vertex[1]),
        FaceDirection::East => (16.0 - vertex[2], 16.0 - vertex[1]),
    }
}

/// Build the six faces of a box using Minecraft's standard box unwrap.
/// `texture_origin` and `texture_size` are in texture pixels (16 for block
/// models, 64 for chest entity textures).
pub(crate) fn box_quads(
    from: [f32; 3],
    to: [f32; 3],
    texture_origin: [f32; 2],
    texture_size: f32,
    texture: u16,
) -> Vec<ModelQuad> {
    let width = to[0] - from[0];
    let height = to[1] - from[1];
    let depth = to[2] - from[2];
    let [u, v] = texture_origin;
    let d = depth;

    let faces = [
        (FaceDirection::East, [u, v + d, u + d, v + d + height]),
        (
            FaceDirection::North,
            [u + d, v + d, u + d + width, v + d + height],
        ),
        (
            FaceDirection::West,
            [u + d + width, v + d, u + d + width + d, v + d + height],
        ),
        (
            FaceDirection::South,
            [
                u + d + width + d,
                v + d,
                u + d + width + d + width,
                v + d + height,
            ],
        ),
        // Entity textures place the visible lid top in the second square.
        (
            FaceDirection::Up,
            [u + d + width, v, u + d + width + width, v + d],
        ),
        (FaceDirection::Down, [u + d, v, u + d + width, v + d]),
    ];

    let mut quads = Vec::with_capacity(6);
    for (face, rect) in faces {
        let vertices = face_vertices(face, from, to);
        let uv = map_uv_rect(face, &vertices, rect, texture_size);
        let Some(normal) = geometric_normal(vertices) else {
            continue;
        };
        quads.push(ModelQuad {
            vertices,
            normal,
            face,
            cullface: None,
            uv,
            texture,
            tint: litematic_mesh::Rgba::WHITE,
            layer: None,
            shade: true,
        });
    }
    quads
}

fn rotate_uvs(uv: &mut [[f32; 2]; 4], degrees: i32) {
    let degrees = degrees.rem_euclid(360);
    if degrees == 0 {
        return;
    }

    let center_u = uv.iter().map(|item| item[0]).sum::<f32>() / 4.0;
    let center_v = uv.iter().map(|item| item[1]).sum::<f32>() / 4.0;

    for item in uv {
        let du = item[0] - center_u;
        let dv = item[1] - center_v;
        let (du, dv) = match degrees {
            90 => (dv, -du),
            180 => (-du, -dv),
            270 => (-dv, du),
            _ => (du, dv),
        };
        item[0] = center_u + du;
        item[1] = center_v + dv;
    }
}

fn rotate_element(vertex: [f32; 3], rotation: &ElementRotation) -> [f32; 3] {
    let angle = rotation.angle.rem_euclid(360.0);
    if angle == 0.0 || angle == 360.0 {
        return vertex;
    }

    let mut point = vertex;
    if rotation.rescale {
        let radians = angle.to_radians();
        let factor = 1.0 / radians.cos().abs().max(radians.sin().abs());
        scale_axes(&mut point, rotation.origin, rotation.axis.as_str(), factor);
    }

    rotate_around(&point, rotation.origin, rotation.axis.as_str(), angle)
}

fn scale_axes(point: &mut [f32; 3], origin: [f32; 3], axis: &str, factor: f32) {
    for index in 0..3 {
        let is_axis = matches!((axis, index), ("x", 0) | ("y", 1) | ("z", 2));
        if !is_axis {
            point[index] = origin[index] + (point[index] - origin[index]) * factor;
        }
    }
}

pub(crate) fn rotate_block(vertex: [f32; 3], x_rotation: i32, y_rotation: i32) -> [f32; 3] {
    let origin = [8.0, 8.0, 8.0];
    let vertex = rotate_around(&vertex, origin, "x", x_rotation as f32);
    rotate_around(&vertex, origin, "y", y_rotation as f32)
}

fn rotate_around(point: &[f32; 3], origin: [f32; 3], axis: &str, degrees: f32) -> [f32; 3] {
    let radians = degrees.to_radians();
    let (sin, cos) = (radians.sin(), radians.cos());
    let x = point[0] - origin[0];
    let y = point[1] - origin[1];
    let z = point[2] - origin[2];

    let (x, y, z) = match axis {
        "x" => (x, y * cos - z * sin, y * sin + z * cos),
        "y" => (x * cos + z * sin, y, -x * sin + z * cos),
        "z" => (x * cos - y * sin, x * sin + y * cos, z),
        _ => (x, y, z),
    };

    [x + origin[0], y + origin[1], z + origin[2]]
}

pub(crate) fn rotate_normal(normal: [f32; 3], x_rotation: i32, y_rotation: i32) -> [f32; 3] {
    let rotated = rotate_around(&normal, [0.0, 0.0, 0.0], "x", x_rotation as f32);
    rotate_around(&rotated, [0.0, 0.0, 0.0], "y", y_rotation as f32)
}

fn rotate_face(face: FaceDirection, x_rotation: i32, y_rotation: i32) -> Option<FaceDirection> {
    if x_rotation.rem_euclid(90) != 0 || y_rotation.rem_euclid(90) != 0 {
        return None;
    }
    let normal = rotate_normal(face.normal(), x_rotation, y_rotation);
    nearest_face(normal)
}

fn nearest_face(normal: [f32; 3]) -> Option<FaceDirection> {
    let mut best = None;
    let mut best_dot = 0.9f32;
    for face in FaceDirection::ALL {
        let dot = face.normal()[0] * normal[0]
            + face.normal()[1] * normal[1]
            + face.normal()[2] * normal[2];
        if dot > best_dot {
            best_dot = dot;
            best = Some(face);
        }
    }
    best
}

fn geometric_normal(vertices: [[f32; 3]; 4]) -> Option<[f32; 3]> {
    let edge_a = [
        vertices[1][0] - vertices[0][0],
        vertices[1][1] - vertices[0][1],
        vertices[1][2] - vertices[0][2],
    ];
    let edge_b = [
        vertices[2][0] - vertices[0][0],
        vertices[2][1] - vertices[0][1],
        vertices[2][2] - vertices[0][2],
    ];
    let normal = [
        edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
        edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
        edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
    ];
    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if length <= 1e-9 {
        return None;
    }
    Some([normal[0] / length, normal[1] / length, normal[2] / length])
}

/// Resolve `#variable` texture references, including sprite objects and
/// `force_translucent`.
pub fn resolve_texture(
    name: &str,
    textures: &BTreeMap<String, TextureRef>,
) -> Option<ResolvedTexture> {
    resolve_texture_inner(name, textures, 0)
}

fn resolve_texture_inner(
    name: &str,
    textures: &BTreeMap<String, TextureRef>,
    depth: usize,
) -> Option<ResolvedTexture> {
    if depth > 32 {
        return None;
    }

    let Some(variable) = name.strip_prefix('#') else {
        return Some(ResolvedTexture {
            path: normalize_texture_name(name),
            force_translucent: false,
        });
    };

    let reference = textures.get(variable)?;
    match reference {
        TextureRef::Path(path) => {
            if path.starts_with('#') {
                resolve_texture_inner(path, textures, depth + 1)
            } else {
                Some(ResolvedTexture {
                    path: normalize_texture_name(path),
                    force_translucent: false,
                })
            }
        }
        TextureRef::Sprite(sprite) => {
            if sprite.sprite.starts_with('#') {
                let mut resolved = resolve_texture_inner(&sprite.sprite, textures, depth + 1)?;
                resolved.force_translucent |= sprite.force_translucent;
                Some(resolved)
            } else {
                Some(ResolvedTexture {
                    path: normalize_texture_name(&sprite.sprite),
                    force_translucent: sprite.force_translucent,
                })
            }
        }
    }
}

fn normalize_texture_name(path: &str) -> String {
    let path = path.strip_prefix("minecraft:").unwrap_or(path);
    path.trim_start_matches("textures/").to_string()
}
