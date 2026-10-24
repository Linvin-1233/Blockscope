use crate::color::{RenderLayer, Rgba};
use std::sync::{Arc, OnceLock};

/// Sentinel for geometry that uses the block color instead of a texture.
pub const NO_TEXTURE: u16 = u16::MAX;

/// The six axis-aligned face directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FaceDirection {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl FaceDirection {
    pub const ALL: [Self; 6] = [
        Self::Down,
        Self::Up,
        Self::North,
        Self::South,
        Self::West,
        Self::East,
    ];

    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::Down => [0.0, -1.0, 0.0],
            Self::Up => [0.0, 1.0, 0.0],
            Self::North => [0.0, 0.0, -1.0],
            Self::South => [0.0, 0.0, 1.0],
            Self::West => [-1.0, 0.0, 0.0],
            Self::East => [1.0, 0.0, 0.0],
        }
    }

    pub const fn offset(self) -> [i64; 3] {
        match self {
            Self::Down => [0, -1, 0],
            Self::Up => [0, 1, 0],
            Self::North => [0, 0, -1],
            Self::South => [0, 0, 1],
            Self::West => [-1, 0, 0],
            Self::East => [1, 0, 0],
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Down => "down",
            Self::Up => "up",
            Self::North => "north",
            Self::South => "south",
            Self::West => "west",
            Self::East => "east",
        }
    }

    /// Tangential axes used for AO sampling: (u, v).
    pub const fn tangents(self) -> (usize, usize) {
        match self {
            Self::Down | Self::Up => (0, 2),
            Self::North | Self::South => (0, 1),
            Self::West | Self::East => (2, 1),
        }
    }
}

/// A box in 0..16 model units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelBox {
    pub min: [u8; 3],
    pub max: [u8; 3],
}

impl ModelBox {
    pub const fn new(min: [u8; 3], max: [u8; 3]) -> Self {
        Self { min, max }
    }
}

/// A quad in 0..16 model units. Vertices are CCW when viewed from outside.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelQuad {
    pub vertices: [[f32; 3]; 4],
    pub normal: [f32; 3],
    pub face: FaceDirection,
    pub cullface: Option<FaceDirection>,
    pub uv: [[f32; 2]; 4],
    pub texture: u16,
    pub tint: Rgba,
    pub layer: Option<RenderLayer>,
    pub shade: bool,
}

/// A resolved block model.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockModel {
    pub quads: Vec<ModelQuad>,
    pub is_full_cube: bool,
    pub ambient_occlusion: bool,
}

impl BlockModel {
    pub fn occludes(&self, layer: RenderLayer) -> bool {
        self.is_full_cube && layer == RenderLayer::Opaque
    }
}

static FULL_CUBE: OnceLock<Arc<BlockModel>> = OnceLock::new();
static EMPTY: OnceLock<Arc<BlockModel>> = OnceLock::new();

pub fn full_cube_model() -> Arc<BlockModel> {
    FULL_CUBE
        .get_or_init(|| {
            Arc::new(BlockModel {
                quads: full_cube_quads(),
                is_full_cube: true,
                ambient_occlusion: true,
            })
        })
        .clone()
}

pub fn empty_model() -> Arc<BlockModel> {
    EMPTY
        .get_or_init(|| {
            Arc::new(BlockModel {
                quads: Vec::new(),
                is_full_cube: false,
                ambient_occlusion: true,
            })
        })
        .clone()
}

fn full_cube_quads() -> Vec<ModelQuad> {
    vec![
        quad(
            FaceDirection::Down,
            [
                [0.0, 0.0, 16.0],
                [0.0, 0.0, 0.0],
                [16.0, 0.0, 0.0],
                [16.0, 0.0, 16.0],
            ],
        ),
        quad(
            FaceDirection::Up,
            [
                [0.0, 16.0, 0.0],
                [0.0, 16.0, 16.0],
                [16.0, 16.0, 16.0],
                [16.0, 16.0, 0.0],
            ],
        ),
        quad(
            FaceDirection::North,
            [
                [0.0, 0.0, 0.0],
                [0.0, 16.0, 0.0],
                [16.0, 16.0, 0.0],
                [16.0, 0.0, 0.0],
            ],
        ),
        quad(
            FaceDirection::South,
            [
                [16.0, 0.0, 16.0],
                [16.0, 16.0, 16.0],
                [0.0, 16.0, 16.0],
                [0.0, 0.0, 16.0],
            ],
        ),
        quad(
            FaceDirection::West,
            [
                [0.0, 0.0, 16.0],
                [0.0, 16.0, 16.0],
                [0.0, 16.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
        ),
        quad(
            FaceDirection::East,
            [
                [16.0, 0.0, 0.0],
                [16.0, 16.0, 0.0],
                [16.0, 16.0, 16.0],
                [16.0, 0.0, 16.0],
            ],
        ),
    ]
}

fn quad(face: FaceDirection, vertices: [[f32; 3]; 4]) -> ModelQuad {
    ModelQuad {
        vertices,
        normal: face.normal(),
        face,
        cullface: Some(face),
        uv: face_uv(face, vertices),
        texture: NO_TEXTURE,
        tint: Rgba::WHITE,
        layer: None,
        shade: true,
    }
}

/// Derive 0..1 UVs from model coordinates, matching Minecraft's texture
/// orientation for axis-aligned faces.
pub fn face_uv(face: FaceDirection, vertices: [[f32; 3]; 4]) -> [[f32; 2]; 4] {
    let mut uv = [[0.0f32; 2]; 4];
    for (index, vertex) in vertices.iter().enumerate() {
        let (u, v) = match face {
            FaceDirection::Down => (vertex[0], 16.0 - vertex[2]),
            FaceDirection::Up => (vertex[0], vertex[2]),
            FaceDirection::North => (16.0 - vertex[0], 16.0 - vertex[1]),
            FaceDirection::South => (vertex[0], 16.0 - vertex[1]),
            FaceDirection::West => (vertex[2], 16.0 - vertex[1]),
            FaceDirection::East => (16.0 - vertex[2], 16.0 - vertex[1]),
        };
        uv[index] = [u / 16.0, v / 16.0];
    }
    uv
}
