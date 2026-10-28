use super::super::RenderStatistics;
use super::super::bin::worker_count;
use super::super::project::{Triangle, project_pages};
use super::super::stream::chunk_ranges;
use super::vertex::{self, GpuVertex, Viewport};
use super::{GpuFailure, atlas, resolve};
use crate::{Camera, RenderError, RenderOptions, RenderOutput};
use litematic_material::{TextureFilter, TextureSet};
use litematic_mesh::Mesh;
use litematic_scene::Scene;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use wgpu::util::DeviceExt;

const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const TRANSLUCENT_BATCH_TRIANGLES: usize = 1_000_000;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    filter_mode: u32,
    _padding: [u32; 3],
}

/// Process-wide device and pipelines.
///
/// Pipeline creation and shader compilation are expensive and look like render
/// time until the first readback. Caching them once per process makes repeated
/// renders (multi-view jobs, `serve`, tests) pay that cost once.
struct GpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    bind_group_layout: wgpu::BindGroupLayout,
    opaque_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    errors: ErrorSlot,
}

fn shared_context() -> Result<Arc<GpuContext>, GpuFailure> {
    static CONTEXT: OnceLock<Result<Arc<GpuContext>, String>> = OnceLock::new();
    CONTEXT
        .get_or_init(|| {
            init_context()
                .map(Arc::new)
                .map_err(|failure| failure.into_error().message)
        })
        .clone()
        .map_err(|message| GpuFailure::Unavailable(RenderError::unavailable(message)))
}

fn init_context() -> Result<GpuContext, GpuFailure> {
    let (device, queue, errors) = create_device()?;
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blockscope gpu bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blockscope gpu pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blockscope gpu shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let opaque_pipeline = create_pipeline(
        &device,
        &pipeline_layout,
        &shader,
        "blockscope gpu opaque",
        None,
        true,
    );
    let translucent_pipeline = create_pipeline(
        &device,
        &pipeline_layout,
        &shader,
        "blockscope gpu translucent",
        Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        false,
    );
    if let Some(message) = take_error(&errors) {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
            "GPU pipeline initialization failed: {message}"
        ))));
    }
    Ok(GpuContext {
        device,
        queue,
        bind_group_layout,
        opaque_pipeline,
        translucent_pipeline,
        errors,
    })
}

pub(super) fn render(
    scene: &Scene,
    mesh: &Mesh,
    camera: &Camera,
    options: &RenderOptions,
    textures: Option<&TextureSet>,
) -> Result<RenderOutput, GpuFailure> {
    let internal_width = options.internal_width();
    let internal_height = options.internal_height();
    let context = shared_context()?;
    let device = &context.device;
    let queue = &context.queue;
    // A previous render may have failed before its readback drained the slot.
    let _ = take_error(&context.errors);

    let limits = device.limits();
    if internal_width > limits.max_texture_dimension_2d
        || internal_height > limits.max_texture_dimension_2d
    {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
            "output {internal_width}x{internal_height} exceeds the GPU texture limit {}",
            limits.max_texture_dimension_2d
        ))));
    }

    let row_stride = align_to(
        internal_width as usize * 4,
        wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize,
    );
    let readback_bytes = row_stride as u64 * u64::from(internal_height);
    if readback_bytes > limits.max_buffer_size {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
            "readback needs {readback_bytes} bytes, GPU buffer limit is {}",
            limits.max_buffer_size
        ))));
    }

    let atlas =
        atlas::build(textures, limits.max_texture_dimension_2d).map_err(GpuFailure::Unavailable)?;
    let atlas_texture = upload_texture(device, queue, atlas.width, atlas.height, &atlas.rgba);
    let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut rect_data = atlas.rects;
    if rect_data.is_empty() {
        rect_data.push([0; 4]);
    }
    let rect_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("blockscope gpu texture rects"),
        contents: bytemuck::cast_slice(&rect_data),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let filter = match options.texture_filter {
        TextureFilter::Nearest => 0,
        TextureFilter::Linear => 1,
    };
    let globals = Globals {
        filter_mode: filter,
        _padding: [0; 3],
    };
    let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("blockscope gpu globals"),
        contents: bytemuck::bytes_of(&globals),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blockscope gpu bind group"),
        layout: &context.bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: globals_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: rect_buffer.as_entire_binding(),
            },
        ],
    });

    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blockscope gpu color"),
        size: wgpu::Extent3d {
            width: internal_width,
            height: internal_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: COLOR_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blockscope gpu depth"),
        size: wgpu::Extent3d {
            width: internal_width,
            height: internal_height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());

    clear_targets(device, queue, &color_view, &depth_view, options.background);

    let (depth_min, depth_max) = depth_range(scene, camera);
    let viewport = Viewport::new(internal_width, internal_height, depth_min, depth_max);
    let mut statistics = RenderStatistics {
        width: options.width,
        height: options.height,
        supersample: options.supersample,
        internal_width,
        internal_height,
        backend_used: crate::RenderBackend::Gpu,
        ..Default::default()
    };
    let mut translucent: Vec<Triangle> = Vec::new();

    for range in chunk_ranges(&mesh.pages) {
        let chunk = &mesh.pages[range];
        let threads = worker_count(options.threads, chunk.len());
        let (quads, chunk_translucent) =
            project_pages(chunk, camera, options, &mut statistics, threads);
        translucent.extend(chunk_translucent);

        if quads.is_empty() {
            continue;
        }
        let mut vertices = Vec::with_capacity(quads.len() * 6);
        for quad in &quads {
            vertex::push_quad(&mut vertices, quad, &viewport);
        }
        let buffer = vertex_buffer(device, &vertices)?;
        draw(
            device,
            queue,
            &color_view,
            &depth_view,
            &context.opaque_pipeline,
            &bind_group,
            &buffer,
            vertices.len(),
            false,
        );
    }

    translucent.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));
    for batch in translucent.chunks(TRANSLUCENT_BATCH_TRIANGLES) {
        let mut vertices = Vec::with_capacity(batch.len() * 3);
        for triangle in batch {
            vertex::push_triangle(&mut vertices, triangle, &viewport);
        }
        let buffer = vertex_buffer(device, &vertices)?;
        draw(
            device,
            queue,
            &color_view,
            &depth_view,
            &context.translucent_pipeline,
            &bind_group,
            &buffer,
            vertices.len(),
            true,
        );
    }

    let readback = read_back(
        device,
        queue,
        &color,
        internal_width,
        internal_height,
        row_stride as u32,
        readback_bytes,
    )?;
    if let Some(message) = take_error(&context.errors) {
        return Err(GpuFailure::Failed(RenderError::gpu(message)));
    }

    let rgba = resolve::resolve(
        &readback,
        row_stride,
        internal_width,
        options.width,
        options.height,
        options.supersample,
        options.threads as usize,
    );
    Ok(RenderOutput {
        width: options.width,
        height: options.height,
        rgba,
        statistics,
    })
}

type ErrorSlot = Arc<Mutex<Option<String>>>;

fn take_error(errors: &ErrorSlot) -> Option<String> {
    errors.lock().ok().and_then(|mut slot| slot.take())
}

fn create_device() -> Result<(wgpu::Device, wgpu::Queue, ErrorSlot), GpuFailure> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .map_err(|error| {
        GpuFailure::Unavailable(RenderError::unavailable(format!("no GPU adapter: {error}")))
    })?;

    if adapter.get_info().device_type == wgpu::DeviceType::Cpu {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(
            "only a software GPU adapter is available",
        )));
    }

    let limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("blockscope gpu device"),
        required_features: wgpu::Features::empty(),
        required_limits: limits,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| {
        GpuFailure::Unavailable(RenderError::unavailable(format!(
            "cannot create GPU device: {error}"
        )))
    })?;

    let errors = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&errors);
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        if let Ok(mut slot) = slot.lock()
            && slot.is_none()
        {
            *slot = Some(error.to_string());
        }
    }));

    Ok((device, queue, errors))
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    blend: Option<wgpu::BlendState>,
    depth_write_enabled: bool,
) -> wgpu::RenderPipeline {
    let buffers = [Some(vertex::layout())];
    let targets = [Some(wgpu::ColorTargetState {
        format: COLOR_FORMAT,
        blend,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(depth_write_enabled),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("blockscope gpu atlas"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

fn clear_targets(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    background: crate::Background,
) {
    // The color attachment holds premultiplied alpha, so a solid background is
    // scaled by its own alpha before clearing.
    let clear = match background {
        crate::Background::Transparent => wgpu::Color::TRANSPARENT,
        crate::Background::Solid(color) => {
            let alpha = f64::from(color.a) / 255.0;
            wgpu::Color {
                r: f64::from(color.r) / 255.0 * alpha,
                g: f64::from(color.g) / 255.0 * alpha,
                b: f64::from(color.b) / 255.0 * alpha,
                a: alpha,
            }
        }
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu clear"),
    });
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("blockscope gpu clear"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: color,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view: depth,
            depth_ops: Some(wgpu::Operations {
                load: wgpu::LoadOp::Clear(1.0),
                store: wgpu::StoreOp::Store,
            }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    queue.submit(Some(encoder.finish()));
}

fn vertex_buffer(
    device: &wgpu::Device,
    vertices: &[GpuVertex],
) -> Result<wgpu::Buffer, GpuFailure> {
    let bytes = std::mem::size_of_val(vertices) as u64;
    if bytes > device.limits().max_buffer_size {
        return Err(GpuFailure::Failed(RenderError::gpu(format!(
            "vertex batch needs {bytes} bytes, GPU buffer limit is {}",
            device.limits().max_buffer_size
        ))));
    }
    Ok(
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blockscope gpu vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    buffer: &wgpu::Buffer,
    vertex_count: usize,
    translucent: bool,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu draw"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(if translucent {
                "blockscope gpu translucent"
            } else {
                "blockscope gpu opaque"
            }),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..vertex_count as u32, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

fn read_back(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::Texture,
    width: u32,
    height: u32,
    row_stride: u32,
    size: u64,
) -> Result<Vec<u8>, GpuFailure> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blockscope gpu readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: color,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row_stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .map_err(|error| {
            GpuFailure::Failed(RenderError::gpu(format!("GPU poll failed: {error}")))
        })?;
    match receiver.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(GpuFailure::Failed(RenderError::gpu(format!(
                "GPU readback mapping failed: {error}"
            ))));
        }
        Err(error) => {
            return Err(GpuFailure::Failed(RenderError::gpu(format!(
                "GPU readback callback failed: {error}"
            ))));
        }
    }

    let data = slice
        .get_mapped_range()
        .map_err(|error| {
            GpuFailure::Failed(RenderError::gpu(format!(
                "GPU readback view failed: {error}"
            )))
        })?
        .to_vec();
    buffer.unmap();
    Ok(data)
}

fn depth_range(scene: &Scene, camera: &Camera) -> (f64, f64) {
    let min = scene.bounds.min;
    let max = scene.bounds.max_exclusive;
    let mut near = f64::INFINITY;
    let mut far = f64::NEG_INFINITY;
    for x in [min[0], max[0]] {
        for y in [min[1], max[1]] {
            for z in [min[2], max[2]] {
                let depth = camera.project([x as f64, y as f64, z as f64])[2];
                near = near.min(depth);
                far = far.max(depth);
            }
        }
    }
    (near, far)
}

fn align_to(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}
