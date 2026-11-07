use super::GpuFailure;
use super::budget;
use super::vertex::{self, GpuVertex};
use crate::{Camera, RenderError, RenderOptions, RenderOutput};
use litematic_material::TextureSet;
use litematic_mesh::Mesh;
use litematic_scene::Scene;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use wgpu::util::DeviceExt;

pub(super) const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub(super) const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub(super) const WBOIT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub(super) const TRANSLUCENT_BATCH_TRIANGLES: usize = 1_000_000;
/// Upper bound for one instance upload. Larger page chunks are split so the
/// three-slot staging ring stays inside the documented GPU working budget.
pub(super) const MAX_INSTANCE_BATCH_BYTES: u64 = 32 * 1024 * 1024;

/// Process-wide device and pipelines.
///
/// Pipeline creation and shader compilation are expensive and look like render
/// time until the first readback. Caching them once per process makes repeated
/// renders (multi-view jobs, `serve`, tests) pay that cost once.
pub(super) struct GpuContext {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    pub(super) camera_layout: wgpu::BindGroupLayout,
    pub(super) wboit_composite_layout: wgpu::BindGroupLayout,
    pub(super) instance_pipeline: wgpu::RenderPipeline,
    pub(super) translucent_pipeline: wgpu::RenderPipeline,
    pub(super) wboit_pipeline: wgpu::RenderPipeline,
    pub(super) wboit_composite_pipeline: wgpu::RenderPipeline,
    pub(super) wboit_supported: bool,
    pub(super) working_set_bytes: u64,
    pub(super) errors: ErrorSlot,
}

static CONTEXT: OnceLock<Result<Arc<GpuContext>, String>> = OnceLock::new();

pub(super) fn shared_context() -> Result<Arc<GpuContext>, GpuFailure> {
    CONTEXT
        .get_or_init(|| {
            init_context()
                .map(Arc::new)
                .map_err(|failure| failure.into_error().message)
        })
        .clone()
        .map_err(|message| GpuFailure::Unavailable(RenderError::unavailable(message)))
}

pub(super) fn context_initialized() -> bool {
    CONTEXT.get().is_some_and(|context| context.is_ok())
}

/// Probe the adapter once for the backend policy. This does not create a
/// device or pipelines, so a CPU-only job never pays shader compilation.
pub(super) fn adapter_profile() -> Option<super::AdapterProfile> {
    static PROFILE: OnceLock<Option<super::AdapterProfile>> = OnceLock::new();
    *PROFILE.get_or_init(|| {
        let adapter = request_adapter().ok()?;
        let info = adapter.get_info();
        let wboit_supported = wboit_supported(&adapter);
        Some(super::AdapterProfile {
            software: info.device_type == wgpu::DeviceType::Cpu,
            working_set_bytes: budget::working_set_bytes(budget::detect_vram_bytes(
                info.device_type,
            )),
            wboit_supported,
        })
    })
}

fn init_context() -> Result<GpuContext, GpuFailure> {
    let (device, queue, wboit_supported, working_set_bytes, errors) = create_device()?;
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
    let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("blockscope gpu camera layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let wboit_composite_layout =
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blockscope gpu wboit composite layout"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
    let wboit_composite_pipeline_layout =
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blockscope gpu wboit composite pipeline layout"),
            bind_group_layouts: &[Some(&wboit_composite_layout)],
            immediate_size: 0,
        });
    let instance_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("blockscope gpu instance pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout), Some(&camera_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blockscope gpu shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });
    let instance_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blockscope gpu instance shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("instance.wgsl").into()),
    });
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
        &[vertex::layout()],
    );
    let instance_pipeline = create_pipeline(
        &device,
        &instance_pipeline_layout,
        &instance_shader,
        "blockscope gpu instance",
        None,
        true,
        &[super::instance::layout()],
    );
    let wboit_pipeline =
        create_wboit_pipeline(&device, &pipeline_layout, &shader, &[vertex::layout()]);
    let wboit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("blockscope gpu wboit composite shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("wboit.wgsl").into()),
    });
    let wboit_composite_pipeline =
        create_composite_pipeline(&device, &wboit_composite_pipeline_layout, &wboit_shader);
    if let Some(message) = take_error(&errors) {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
            "GPU pipeline initialization failed: {message}"
        ))));
    }
    Ok(GpuContext {
        device,
        queue,
        bind_group_layout,
        camera_layout,
        wboit_composite_layout,
        instance_pipeline,
        translucent_pipeline,
        wboit_pipeline,
        wboit_composite_pipeline,
        wboit_supported,
        working_set_bytes,
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
    let mut session = super::GpuRenderSession::new_with_camera(scene, camera, options, textures)?;
    session.render_pages_inner(&mesh.pages)?;
    session.finish_inner()
}

pub(super) type ErrorSlot = Arc<Mutex<Option<String>>>;

pub(super) fn take_error(errors: &ErrorSlot) -> Option<String> {
    errors.lock().ok().and_then(|mut slot| slot.take())
}

fn create_device() -> Result<(wgpu::Device, wgpu::Queue, bool, u64, ErrorSlot), GpuFailure> {
    let adapter = request_adapter()?;
    let info = adapter.get_info();
    if info.device_type == wgpu::DeviceType::Cpu {
        return Err(GpuFailure::Unavailable(RenderError::unavailable(
            "only a software GPU adapter is available",
        )));
    }
    let wboit_supported = wboit_supported(&adapter);
    let working_set_bytes = budget::working_set_bytes(budget::detect_vram_bytes(info.device_type));

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

    Ok((device, queue, wboit_supported, working_set_bytes, errors))
}

fn request_adapter() -> Result<wgpu::Adapter, GpuFailure> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    }))
    .map_err(|error| {
        GpuFailure::Unavailable(RenderError::unavailable(format!("no GPU adapter: {error}")))
    })
}

fn wboit_supported(adapter: &wgpu::Adapter) -> bool {
    let features = adapter.get_texture_format_features(WBOIT_FORMAT);
    features
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        && features
            .flags
            .contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    blend: Option<wgpu::BlendState>,
    depth_write_enabled: bool,
    buffers: &[wgpu::VertexBufferLayout<'static>],
) -> wgpu::RenderPipeline {
    let buffers: Vec<Option<wgpu::VertexBufferLayout<'static>>> =
        buffers.iter().cloned().map(Some).collect();
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

pub(super) fn clear_targets(
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

pub(super) fn vertex_buffer(
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

fn create_wboit_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    buffers: &[wgpu::VertexBufferLayout<'static>],
) -> wgpu::RenderPipeline {
    let buffers: Vec<Option<wgpu::VertexBufferLayout<'static>>> =
        buffers.iter().cloned().map(Some).collect();
    let targets = [
        Some(wgpu::ColorTargetState {
            format: WBOIT_FORMAT,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        }),
        Some(wgpu::ColorTargetState {
            format: WBOIT_FORMAT,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::Zero,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            }),
            write_mask: wgpu::ColorWrites::ALL,
        }),
    ];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blockscope gpu wboit"),
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
            depth_write_enabled: Some(false),
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
            entry_point: Some("fs_wboit"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &targets,
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn create_composite_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let targets = [Some(wgpu::ColorTargetState {
        format: COLOR_FORMAT,
        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
        write_mask: wgpu::ColorWrites::ALL,
    })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("blockscope gpu wboit composite"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
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
        depth_stencil: None,
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

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    atlas_bind_group: &wgpu::BindGroup,
    camera_bind_group: &wgpu::BindGroup,
    buffer: &wgpu::Buffer,
    instance_count: usize,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu instance draw"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blockscope gpu instance"),
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
        pass.set_bind_group(0, atlas_bind_group, &[]);
        pass.set_bind_group(1, camera_bind_group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..6, 0..instance_count as u32);
    }
    queue.submit(Some(encoder.finish()));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw(
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

pub(super) fn clear_wboit_targets(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    accum: &wgpu::TextureView,
    reveal: &wgpu::TextureView,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu wboit clear"),
    });
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("blockscope gpu wboit clear"),
        color_attachments: &[
            Some(wgpu::RenderPassColorAttachment {
                view: accum,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            }),
            Some(wgpu::RenderPassColorAttachment {
                view: reveal,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            }),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    queue.submit(Some(encoder.finish()));
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_wboit(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    accum: &wgpu::TextureView,
    reveal: &wgpu::TextureView,
    depth: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    buffer: &wgpu::Buffer,
    vertex_count: usize,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu wboit draw"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blockscope gpu wboit"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view: accum,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
                Some(wgpu::RenderPassColorAttachment {
                    view: reveal,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
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

pub(super) fn composite_wboit(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
) {
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("blockscope gpu wboit composite"),
    });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blockscope gpu wboit composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit(Some(encoder.finish()));
}

pub(super) fn create_readback_buffer(device: &wgpu::Device, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blockscope gpu readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    })
}

/// Copy the color target into `buffer`, map it, and hand the mapped bytes to
/// `resolve`. The caller owns the buffer so a frame session can reuse it.
#[allow(clippy::too_many_arguments)]
pub(super) fn read_back<T>(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    color: &wgpu::Texture,
    buffer: &wgpu::Buffer,
    width: u32,
    height: u32,
    row_stride: u32,
    resolve: impl FnOnce(&[u8]) -> T,
) -> Result<T, GpuFailure> {
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
            buffer,
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

    let data = slice.get_mapped_range().map_err(|error| {
        GpuFailure::Failed(RenderError::gpu(format!(
            "GPU readback view failed: {error}"
        )))
    })?;
    let result = resolve(&data);
    drop(data);
    buffer.unmap();
    Ok(result)
}

pub(super) fn align_to(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}
