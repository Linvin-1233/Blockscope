use super::GpuFailure;
use super::backend::{self, GpuContext};
use crate::Background;

pub(super) struct WboitTargets {
    pub(super) _accum: wgpu::Texture,
    pub(super) accum_view: wgpu::TextureView,
    pub(super) _reveal: wgpu::Texture,
    pub(super) reveal_view: wgpu::TextureView,
    pub(super) bind_group: wgpu::BindGroup,
}

/// Color/depth/WBOIT targets for one view. Both the streaming session and the
/// reusable frame session own exactly one set per view/frame target.
pub(super) struct RenderTargets {
    pub(super) color: wgpu::Texture,
    pub(super) color_view: wgpu::TextureView,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) wboit: Option<WboitTargets>,
}

pub(super) fn create(
    context: &GpuContext,
    width: u32,
    height: u32,
    background: Background,
) -> Result<RenderTargets, GpuFailure> {
    let device = &context.device;
    let queue = &context.queue;
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blockscope gpu color"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: backend::COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blockscope gpu depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: backend::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    backend::clear_targets(device, queue, &color_view, &depth_view, background);
    let wboit = if context.wboit_supported {
        Some(create_wboit(context, width, height)?)
    } else {
        None
    };
    Ok(RenderTargets {
        color,
        color_view,
        depth_view,
        wboit,
    })
}

fn create_wboit(context: &GpuContext, width: u32, height: u32) -> Result<WboitTargets, GpuFailure> {
    let device = &context.device;
    let create = |label| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: backend::WBOIT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let accum = create("blockscope gpu wboit accumulation");
    let reveal = create("blockscope gpu wboit revealage");
    let accum_view = accum.create_view(&wgpu::TextureViewDescriptor::default());
    let reveal_view = reveal.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blockscope gpu wboit composite bind group"),
        layout: &context.wboit_composite_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&accum_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&reveal_view),
            },
        ],
    });
    Ok(WboitTargets {
        _accum: accum,
        accum_view,
        _reveal: reveal,
        reveal_view,
        bind_group,
    })
}
