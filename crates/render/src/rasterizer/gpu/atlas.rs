use crate::RenderError;
use litematic_material::TextureSet;

/// One texture region inside the atlas. `xy` is the origin and `zw` the size.
pub(super) type AtlasRect = [u32; 4];

pub(super) struct Atlas {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) rgba: Vec<u8>,
    /// Indexed by texture id. Missing ids keep a zero rect, which the shader
    /// treats as an untextured draw.
    pub(super) rects: Vec<AtlasRect>,
}

/// The bounded atlas side length. Texture resources top out at 2048 pixels, so
/// 8192 leaves room for large vanilla packs without a multi-gigabyte upload.
const MAX_ATLAS_SIDE: u32 = 8192;

pub(super) fn build(
    textures: Option<&TextureSet>,
    max_texture_dimension: u32,
) -> Result<Atlas, RenderError> {
    let Some(textures) = textures.filter(|set| !set.is_empty()) else {
        return Ok(Atlas {
            width: 1,
            height: 1,
            rgba: vec![255, 255, 255, 255],
            rects: Vec::new(),
        });
    };

    let limit = max_texture_dimension.min(MAX_ATLAS_SIDE);
    let entries: Vec<(u32, u32)> = (0..textures.len())
        .map(|index| {
            let texture = textures.get(index as u16).expect("texture ids are dense");
            (texture.width, texture.height)
        })
        .collect();

    let packed = pack(&entries, limit).ok_or_else(|| {
        RenderError::unavailable(format!(
            "textures do not fit in a {limit}x{limit} GPU atlas"
        ))
    })?;

    if packed.width == 0 || packed.height == 0 {
        return Err(RenderError::unavailable("GPU atlas has no texels"));
    }

    let mut rgba = vec![0u8; (u64::from(packed.width) * u64::from(packed.height) * 4) as usize];
    let mut rects = vec![[0u32; 4]; textures.len()];

    for (index, origin) in packed.origins.iter().enumerate() {
        let texture = textures.get(index as u16).expect("texture ids are dense");
        let (width, height) = entries[index];
        if origin[0] + width > packed.width || origin[1] + height > packed.height {
            return Err(RenderError::unavailable(
                "GPU atlas packer produced an out-of-bounds rect",
            ));
        }
        let expected = (u64::from(width) * u64::from(height) * 4) as usize;
        if texture.rgba.len() != expected {
            return Err(RenderError::unavailable(format!(
                "texture {index} has {} bytes, expected {expected}",
                texture.rgba.len()
            )));
        }

        for row in 0..height {
            let source_start = (u64::from(row) * u64::from(width) * 4) as usize;
            let source_end = source_start + width as usize * 4;
            let target_start = (u64::from(origin[1] + row) * u64::from(packed.width)
                + u64::from(origin[0])) as usize
                * 4;
            rgba[target_start..target_start + width as usize * 4]
                .copy_from_slice(&texture.rgba[source_start..source_end]);
        }

        rects[index] = [origin[0], origin[1], origin[0] + width, origin[1] + height];
    }

    Ok(Atlas {
        width: packed.width,
        height: packed.height,
        rgba,
        rects,
    })
}

struct Packed {
    width: u32,
    height: u32,
    origins: Vec<[u32; 2]>,
}

/// Shelf-pack textures into the smallest candidate square whose used height
/// still fits. The atlas is allowed to be shorter than it is wide, which keeps
/// a pack with one tall texture from allocating a full square.
fn pack(entries: &[(u32, u32)], limit: u32) -> Option<Packed> {
    if limit == 0 {
        return None;
    }
    let largest = entries
        .iter()
        .map(|&(width, height)| width.max(height))
        .max()
        .unwrap_or(0);
    if largest == 0 || largest > limit {
        return None;
    }

    let mut side = largest.next_power_of_two();
    let mut best: Option<Packed> = None;
    while side <= limit {
        if let Some(candidate) = pack_side(entries, side) {
            let area = u64::from(candidate.width) * u64::from(candidate.height);
            let best_area = best
                .as_ref()
                .map(|packed| u64::from(packed.width) * u64::from(packed.height))
                .unwrap_or(u64::MAX);
            if area < best_area {
                best = Some(candidate);
            }
        }
        if side == limit {
            break;
        }
        side = side.saturating_mul(2).min(limit);
    }
    best
}

fn pack_side(entries: &[(u32, u32)], side: u32) -> Option<Packed> {
    struct Shelf {
        y: u32,
        height: u32,
        x: u32,
    }

    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&left, &right| {
        let left_entry = entries[left];
        let right_entry = entries[right];
        right_entry
            .1
            .cmp(&left_entry.1)
            .then_with(|| right_entry.0.cmp(&left_entry.0))
            .then_with(|| left.cmp(&right))
    });

    let mut origins = vec![[0u32; 2]; entries.len()];
    let mut shelves: Vec<Shelf> = Vec::new();

    for index in order {
        let (width, height) = entries[index];
        if width > side || height > side {
            return None;
        }

        // Best vertical fit keeps short shelves available for short textures
        // instead of widening the first tall shelf.
        let shelf = shelves
            .iter_mut()
            .filter(|shelf| shelf.height >= height && shelf.x + width <= side)
            .min_by_key(|shelf| shelf.height - height);
        let position = if let Some(shelf) = shelf {
            let position: [u32; 2] = [shelf.x, shelf.y];
            shelf.x += width;
            position
        } else {
            let y = shelves
                .last()
                .map(|shelf| shelf.y + shelf.height)
                .unwrap_or(0);
            if y + height > side {
                return None;
            }
            shelves.push(Shelf {
                y,
                height,
                x: width,
            });
            [0, y]
        };
        origins[index] = position;
    }

    let height = shelves
        .last()
        .map(|shelf| shelf.y + shelf.height)
        .unwrap_or(0);
    (height > 0 && height <= side).then_some(Packed {
        width: side,
        height,
        origins,
    })
}

#[cfg(test)]
mod tests {
    use super::{pack, pack_side};

    #[test]
    fn packs_small_textures_without_overlap() {
        let entries = [(16, 16), (16, 16), (16, 16), (16, 16), (32, 8)];
        let packed = pack(&entries, 64).expect("fits");
        assert_eq!(packed.origins.len(), entries.len());
        for (index, origin) in packed.origins.iter().enumerate() {
            let (width, height) = entries[index];
            assert!(origin[0] + width <= packed.width);
            assert!(origin[1] + height <= packed.height);
            for (other, other_origin) in packed.origins.iter().enumerate().skip(index + 1) {
                let (other_width, other_height) = entries[other];
                let separated = origin[0] + width <= other_origin[0]
                    || other_origin[0] + other_width <= origin[0]
                    || origin[1] + height <= other_origin[1]
                    || other_origin[1] + other_height <= origin[1];
                assert!(separated, "rect {index} overlaps rect {other}");
            }
        }
    }

    #[test]
    fn rejects_textures_larger_than_the_limit() {
        assert!(pack(&[(2048, 2048)], 1024).is_none());
        assert!(pack(&[(16, 16)], 0).is_none());
    }

    #[test]
    fn side_pack_uses_a_single_shelf_when_it_fits() {
        let packed = pack_side(&[(16, 16), (16, 16)], 32).expect("fits");
        assert_eq!(packed.origins.len(), 2);
        assert_eq!(packed.origins[0][1], packed.origins[1][1]);
    }
}
