use crate::catalog::BlockCatalog;
use crate::model::ModelQuad;
use crate::view::SceneBlockView;
use crate::{Mesh, MeshError, MeshLimits, MeshPage, MeshStatistics, Quad, Vertex};
use litematic_scene::{BlockStateKey, Scene};
use std::collections::BTreeMap;

/// Build a page-local mesh from a scene using the supplied catalog.
pub fn build_mesh(
    scene: &Scene,
    catalog: &dyn BlockCatalog,
    limits: &MeshLimits,
) -> Result<Mesh, MeshError> {
    let view = SceneBlockView::new(scene, limits)?;
    let mut pages: BTreeMap<[i32; 3], Vec<Quad>> = BTreeMap::new();
    let mut statistics = MeshStatistics::default();

    for (region_index, region) in scene.regions.iter().enumerate() {
        for y in 0..region.size[1] {
            for z in 0..region.size[2] {
                for x in 0..region.size[0] {
                    let world = [
                        region.origin[0] + i64::from(x),
                        region.origin[1] + i64::from(y),
                        region.origin[2] + i64::from(z),
                    ];

                    let Some(resolved) = view.resolve(world) else {
                        continue;
                    };
                    if resolved.region_index != region_index {
                        continue;
                    }

                    statistics.blocks_visited += 1;
                    if resolved.state.is_air() {
                        continue;
                    }

                    statistics.blocks_rendered += 1;
                    let block = catalog.resolve(resolved.state);
                    if !block.known {
                        statistics.unknown_blocks += 1;
                    }

                    let page_origin = page_origin(world)?;
                    let local = [
                        world[0].rem_euclid(16) as f32,
                        world[1].rem_euclid(16) as f32,
                        world[2].rem_euclid(16) as f32,
                    ];

                    for model_quad in &block.model.quads {
                        if should_cull(&view, catalog, resolved.state, world, model_quad) {
                            continue;
                        }

                        if statistics.quads >= limits.max_quads as u64 {
                            return Err(MeshError::limit(format!(
                                "mesh exceeds {} quads",
                                limits.max_quads
                            )));
                        }
                        if statistics.vertices + 4 > limits.max_vertices as u64 {
                            return Err(MeshError::limit(format!(
                                "mesh exceeds {} vertices",
                                limits.max_vertices
                            )));
                        }
                        if !pages.contains_key(&page_origin) && pages.len() >= limits.max_pages {
                            return Err(MeshError::page_limit(format!(
                                "mesh exceeds {} pages",
                                limits.max_pages
                            )));
                        }

                        statistics.add_quad(block.layer);
                        let vertices = model_quad.vertices.map(|corner| Vertex {
                            position: [
                                local[0] + corner[0] / 16.0,
                                local[1] + corner[1] / 16.0,
                                local[2] + corner[2] / 16.0,
                            ],
                            normal: model_quad.normal,
                            color: block.color.to_array(),
                            uv: [0.0, 0.0],
                        });

                        pages.entry(page_origin).or_default().push(Quad {
                            vertices,
                            layer: block.layer,
                        });
                    }
                }
            }
        }
    }

    statistics.pages = pages.len() as u64;
    let pages = pages
        .into_iter()
        .map(|(origin, quads)| MeshPage { origin, quads })
        .collect();

    Ok(Mesh { pages, statistics })
}

fn should_cull(
    view: &SceneBlockView<'_>,
    catalog: &dyn BlockCatalog,
    state: &BlockStateKey,
    world: [i64; 3],
    quad: &ModelQuad,
) -> bool {
    let Some(cullface) = quad.cullface else {
        return false;
    };

    let offset = cullface.offset();
    let neighbor_position = [
        world[0] + offset[0],
        world[1] + offset[1],
        world[2] + offset[2],
    ];

    let Some(neighbor) = view.resolve(neighbor_position) else {
        return false;
    };
    if neighbor.state.is_air() {
        return false;
    }

    let neighbor_block = catalog.resolve(neighbor.state);
    if neighbor_block.occludes() {
        return true;
    }

    neighbor.state == state
}

fn page_origin(world: [i64; 3]) -> Result<[i32; 3], MeshError> {
    let mut origin = [0i32; 3];
    for (axis, value) in origin.iter_mut().enumerate() {
        let page = world[axis].div_euclid(16) * 16;
        *value = i32::try_from(page).map_err(|_| {
            MeshError::coordinate(format!("page origin {page} does not fit in i32"))
        })?;
    }
    Ok(origin)
}
