use crate::color::Rgba;
use crate::model::{
    BlockModel, FaceDirection, ModelBox, ModelQuad, NO_TEXTURE, face_uv, full_cube_model,
};

const SIZE: usize = 16;
const CELLS: usize = SIZE * SIZE * SIZE;
const WORDS: usize = CELLS / 64;

/// Build a model from boxes and explicit quads. Box faces are merged greedily.
pub(crate) fn build_model(boxes: &[ModelBox], extra_quads: Vec<ModelQuad>) -> BlockModel {
    if boxes.is_empty() {
        return BlockModel {
            quads: extra_quads,
            is_full_cube: false,
            ambient_occlusion: true,
        };
    }

    let mut grid = VoxelGrid::default();
    for model_box in boxes {
        grid.fill(*model_box);
    }

    if extra_quads.is_empty() && grid.all_occupied() {
        return BlockModel {
            quads: full_cube_model().quads.clone(),
            is_full_cube: true,
            ambient_occlusion: true,
        };
    }

    let mut quads = Vec::new();
    for face in FaceDirection::ALL {
        for plane in 0..SIZE as u8 {
            let mut exposed = [[false; SIZE]; SIZE];
            for (u, row) in exposed.iter_mut().enumerate() {
                for (v, cell) in row.iter_mut().enumerate() {
                    let (inside, outside) = cell_coords(face, plane, u as u8, v as u8);
                    if grid.get(inside) && !grid.get(outside) {
                        *cell = true;
                    }
                }
            }
            merge_plane(face, plane, &exposed, &mut quads);
        }
    }
    quads.extend(extra_quads);

    BlockModel {
        quads,
        is_full_cube: false,
        ambient_occlusion: true,
    }
}

#[derive(Debug, Clone)]
struct VoxelGrid {
    bits: [u64; WORDS],
}

impl Default for VoxelGrid {
    fn default() -> Self {
        Self { bits: [0; WORDS] }
    }
}

impl VoxelGrid {
    fn fill(&mut self, model_box: ModelBox) {
        for x in model_box.min[0]..model_box.max[0] {
            for y in model_box.min[1]..model_box.max[1] {
                for z in model_box.min[2]..model_box.max[2] {
                    self.set([x, y, z]);
                }
            }
        }
    }

    fn set(&mut self, position: [u8; 3]) {
        if position.iter().any(|value| *value >= SIZE as u8) {
            return;
        }
        let index =
            position[1] as usize * SIZE * SIZE + position[2] as usize * SIZE + position[0] as usize;
        self.bits[index / 64] |= 1u64 << (index % 64);
    }

    fn get(&self, position: [i32; 3]) -> bool {
        if position
            .iter()
            .any(|value| *value < 0 || *value >= SIZE as i32)
        {
            return false;
        }
        let index =
            position[1] as usize * SIZE * SIZE + position[2] as usize * SIZE + position[0] as usize;
        (self.bits[index / 64] & (1u64 << (index % 64))) != 0
    }

    fn all_occupied(&self) -> bool {
        self.bits.iter().all(|word| *word == u64::MAX)
    }
}

fn cell_coords(face: FaceDirection, plane: u8, u: u8, v: u8) -> ([i32; 3], [i32; 3]) {
    let plane = i32::from(plane);
    let u = i32::from(u);
    let v = i32::from(v);

    match face {
        FaceDirection::Down => ([u, plane, v], [u, plane - 1, v]),
        FaceDirection::Up => ([u, plane, v], [u, plane + 1, v]),
        FaceDirection::North => ([u, v, plane], [u, v, plane - 1]),
        FaceDirection::South => ([u, v, plane], [u, v, plane + 1]),
        FaceDirection::West => ([plane, v, u], [plane - 1, v, u]),
        FaceDirection::East => ([plane, v, u], [plane + 1, v, u]),
    }
}

fn merge_plane(
    face: FaceDirection,
    plane: u8,
    exposed: &[[bool; SIZE]; SIZE],
    quads: &mut Vec<ModelQuad>,
) {
    let mut used = [[false; SIZE]; SIZE];

    for v in 0..SIZE {
        for u in 0..SIZE {
            if !exposed[u][v] || used[u][v] {
                continue;
            }

            let mut width = 1;
            while u + width < SIZE && exposed[u + width][v] && !used[u + width][v] {
                width += 1;
            }

            let mut height = 1;
            'grow: while v + height < SIZE {
                for x in u..u + width {
                    if !exposed[x][v + height] || used[x][v + height] {
                        break 'grow;
                    }
                }
                height += 1;
            }

            for row in used.iter_mut().skip(u).take(width) {
                for cell in row.iter_mut().skip(v).take(height) {
                    *cell = true;
                }
            }

            quads.push(plane_quad(face, plane, u, v, width, height));
        }
    }
}

fn plane_quad(
    face: FaceDirection,
    plane: u8,
    u: usize,
    v: usize,
    width: usize,
    height: usize,
) -> ModelQuad {
    let u0 = u as f32;
    let u1 = (u + width) as f32;
    let v0 = v as f32;
    let v1 = (v + height) as f32;
    let plane = plane as f32;

    let vertices = match face {
        FaceDirection::Down => [
            [u0, plane, v1],
            [u0, plane, v0],
            [u1, plane, v0],
            [u1, plane, v1],
        ],
        FaceDirection::Up => [
            [u0, plane + 1.0, v0],
            [u0, plane + 1.0, v1],
            [u1, plane + 1.0, v1],
            [u1, plane + 1.0, v0],
        ],
        FaceDirection::North => [
            [u0, v0, plane],
            [u0, v1, plane],
            [u1, v1, plane],
            [u1, v0, plane],
        ],
        FaceDirection::South => [
            [u1, v0, plane + 1.0],
            [u1, v1, plane + 1.0],
            [u0, v1, plane + 1.0],
            [u0, v0, plane + 1.0],
        ],
        FaceDirection::West => [
            [plane, v0, u1],
            [plane, v1, u1],
            [plane, v1, u0],
            [plane, v0, u0],
        ],
        FaceDirection::East => [
            [plane + 1.0, v0, u0],
            [plane + 1.0, v1, u0],
            [plane + 1.0, v1, u1],
            [plane + 1.0, v0, u1],
        ],
    };

    let boundary = match face {
        FaceDirection::Down | FaceDirection::North | FaceDirection::West => plane == 0.0,
        FaceDirection::Up | FaceDirection::South | FaceDirection::East => plane == 15.0,
    };

    ModelQuad {
        vertices,
        normal: face.normal(),
        face,
        cullface: boundary.then_some(face),
        uv: face_uv(face, vertices),
        texture: NO_TEXTURE,
        tint: Rgba::WHITE,
        layer: None,
        shade: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_box_becomes_a_full_cube() {
        let model = build_model(&[ModelBox::new([0, 0, 0], [16, 16, 16])], Vec::new());

        assert!(model.is_full_cube);
        assert_eq!(model.quads.len(), 6);
    }

    #[test]
    fn bottom_slab_has_six_faces() {
        let model = build_model(&[ModelBox::new([0, 0, 0], [16, 8, 16])], Vec::new());

        assert!(!model.is_full_cube);
        assert_eq!(model.quads.len(), 6);
    }

    #[test]
    fn internal_faces_are_removed() {
        let model = build_model(
            &[
                ModelBox::new([0, 0, 0], [16, 8, 16]),
                ModelBox::new([8, 8, 0], [16, 16, 16]),
            ],
            Vec::new(),
        );

        assert!(!model.is_full_cube);
        assert_eq!(model.quads.len(), 10);
    }
}
