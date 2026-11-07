use super::super::project::Triangle;
use super::GpuFailure;
use super::atlas;
use super::backend;
use super::targets::WboitTargets;
use super::translucent::TranslucentParts;
use super::vertex::{self, GpuVertex, Viewport};
use litematic_material::TextureFilter;

/// Draw one translucent store's contents. Retained memory parts are already
/// sorted far-to-near; spilled sub-batches are sorted as they are read.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_parts(
    context: &backend::GpuContext,
    atlas: &atlas::CachedAtlas,
    color_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
    wboit: Option<&WboitTargets>,
    viewport: &Viewport,
    filter: TextureFilter,
    parts: TranslucentParts,
) -> Result<(), GpuFailure> {
    match parts {
        TranslucentParts::Memory(triangles) => draw_batches(
            context, atlas, color_view, depth_view, wboit, viewport, filter, &triangles,
        ),
        TranslucentParts::Spilled(mut reader) => loop {
            let mut batch = match reader.read_next() {
                Ok(Some(batch)) => batch,
                Ok(None) => return Ok(()),
                Err(error) => return Err(temp_storage_failure(error)),
            };
            batch.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));
            draw_batches(
                context, atlas, color_view, depth_view, wboit, viewport, filter, &batch,
            )?;
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_batches(
    context: &backend::GpuContext,
    atlas: &atlas::CachedAtlas,
    color_view: &wgpu::TextureView,
    depth_view: &wgpu::TextureView,
    wboit: Option<&WboitTargets>,
    viewport: &Viewport,
    filter: TextureFilter,
    triangles: &[Triangle],
) -> Result<(), GpuFailure> {
    for batch in triangles.chunks(backend::TRANSLUCENT_BATCH_TRIANGLES) {
        let mut vertices: Vec<GpuVertex> = Vec::with_capacity(batch.len() * 3);
        for triangle in batch {
            vertex::push_triangle(&mut vertices, triangle, viewport);
        }
        let buffer = backend::vertex_buffer(&context.device, &vertices)?;
        if let Some(wboit) = wboit {
            backend::draw_wboit(
                &context.device,
                &context.queue,
                &wboit.accum_view,
                &wboit.reveal_view,
                depth_view,
                &context.wboit_pipeline,
                atlas.bind_group(filter),
                &buffer,
                vertices.len(),
            );
        } else {
            backend::draw(
                &context.device,
                &context.queue,
                color_view,
                depth_view,
                &context.translucent_pipeline,
                atlas.bind_group(filter),
                &buffer,
                vertices.len(),
                true,
            );
        }
    }
    Ok(())
}

pub(super) fn temp_storage_failure(error: std::io::Error) -> GpuFailure {
    GpuFailure::Failed(crate::RenderError::temp_storage(format!(
        "GPU translucent spill: {error}"
    )))
}
