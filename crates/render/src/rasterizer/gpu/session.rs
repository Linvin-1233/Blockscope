use super::super::bin::worker_count;
use super::super::project::{Triangle, push_page_translucent};
use super::super::{GpuStageTimings, RenderStatistics};
use super::translucent::TranslucentStore;
use super::vertex::Viewport;
use super::{GpuFailure, atlas, backend, draw, instance, resolve, targets};
use crate::{Camera, RenderError, RenderOptions, RenderOutput};
use litematic_material::TextureSet;
use litematic_mesh::{MeshPage, NO_TEXTURE};
use litematic_scene::Scene;
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

const STAGING_SLOTS: usize = 3;
pub(super) const STAGING_SLOTS_U64: u64 = STAGING_SLOTS as u64;
const MIN_STAGING_BYTES: u64 = 1024 * 1024;
const MAX_INSTANCE_BYTES: usize =
    backend::MAX_INSTANCE_BATCH_BYTES as usize / std::mem::size_of::<instance::GpuQuad>();

struct GpuView {
    camera: Camera,
    options: RenderOptions,
    frame_origin: [i32; 3],
    viewport: Viewport,
    camera_bind_group: wgpu::BindGroup,
    _camera_buffer: wgpu::Buffer,
    targets: targets::RenderTargets,
    statistics: RenderStatistics,
    translucent: TranslucentStore,
}

/// Reusable GPU render session. Page chunks can be fed incrementally; opaque
/// and cutout geometry is drawn per chunk while translucent triangles are
/// retained for the final sorted pass.
pub struct GpuRenderSession {
    context: Arc<backend::GpuContext>,
    atlas: atlas::CachedAtlas,
    views: Vec<GpuView>,
    width: u32,
    height: u32,
    row_stride: u32,
    readback_bytes: u64,
    timings: GpuStageTimings,
    total_started: Instant,
    staging: StagingRing,
    instances: Vec<instance::GpuQuad>,
    translucent_scratch: Vec<Triangle>,
    draws_submitted: u64,
}

impl GpuRenderSession {
    /// Create a single-view session for the supplied scene and options.
    pub fn new(
        scene: &Scene,
        options: &RenderOptions,
        textures: Option<&TextureSet>,
    ) -> Result<Self, RenderError> {
        Self::new_views(scene, std::slice::from_ref(options), textures)
    }

    /// Create a session that renders the same page stream into several views.
    pub fn new_views(
        scene: &Scene,
        options: &[RenderOptions],
        textures: Option<&TextureSet>,
    ) -> Result<Self, RenderError> {
        if options.is_empty() {
            return Err(RenderError::invalid("views must not be empty"));
        }
        let mut views = Vec::with_capacity(options.len());
        for options in options {
            options.validate()?;
            let camera = Camera::new(&scene.bounds, options)?;
            views.push((camera, options.clone()));
        }
        Self::new_views_with_cameras(scene, &views, textures).map_err(GpuFailure::into_error)
    }

    pub(super) fn new_with_camera(
        scene: &Scene,
        camera: &Camera,
        options: &RenderOptions,
        textures: Option<&TextureSet>,
    ) -> Result<Self, GpuFailure> {
        Self::new_views_with_cameras(scene, &[(camera.clone(), options.clone())], textures)
    }

    fn new_views_with_cameras(
        scene: &Scene,
        views: &[(Camera, RenderOptions)],
        textures: Option<&TextureSet>,
    ) -> Result<Self, GpuFailure> {
        let Some((_first_camera, first_options)) = views.first() else {
            return Err(GpuFailure::Unavailable(RenderError::invalid(
                "views must not be empty",
            )));
        };
        let internal_width = first_options.internal_width();
        let internal_height = first_options.internal_height();
        for (_, options) in views {
            if options.internal_width() != internal_width
                || options.internal_height() != internal_height
            {
                return Err(GpuFailure::Unavailable(RenderError::invalid(
                    "all views must share the same internal size",
                )));
            }
        }

        let total_started = Instant::now();
        let mut timings = GpuStageTimings::default();
        let context = backend::shared_context()?;
        let device = &context.device;
        let queue = &context.queue;
        let _ = backend::take_error(&context.errors);

        let limits = device.limits();
        if internal_width > limits.max_texture_dimension_2d
            || internal_height > limits.max_texture_dimension_2d
        {
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "output {internal_width}x{internal_height} exceeds the GPU texture limit {}",
                limits.max_texture_dimension_2d
            ))));
        }

        let row_stride = backend::align_to(
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

        let atlas_started = Instant::now();
        let atlas_budget = super::budget::atlas_cache_budget(context.working_set_bytes);
        let atlas = atlas::get_or_build(
            device,
            queue,
            &context.bind_group_layout,
            textures,
            limits.max_texture_dimension_2d,
            atlas_budget,
        )
        .map_err(GpuFailure::Unavailable)?;
        timings.atlas_us = elapsed_us(atlas_started);
        timings.atlas_cache_hit = atlas.cache_hit;

        let required = super::budget::estimate_session_bytes(
            internal_width,
            internal_height,
            views.len(),
            context.wboit_supported,
            backend::MAX_INSTANCE_BATCH_BYTES,
            STAGING_SLOTS_U64,
            atlas.bytes,
        );
        if required > context.working_set_bytes {
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "GPU working set needs {required} bytes, 60% VRAM budget is {}",
                context.working_set_bytes
            ))));
        }

        let setup_started = Instant::now();
        let mut gpu_views = Vec::with_capacity(views.len());
        for (camera, options) in views {
            let (gpu_camera, frame_origin) =
                super::camera_uniform(scene, camera).map_err(GpuFailure::Unavailable)?;
            let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("blockscope gpu camera"),
                contents: bytemuck::bytes_of(&gpu_camera),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blockscope gpu camera bind group"),
                layout: &context.camera_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }],
            });

            let targets = targets::create(
                &context,
                internal_width,
                internal_height,
                options.background,
            )?;

            let (depth_min, depth_max) = super::depth_range(scene, camera);
            gpu_views.push(GpuView {
                camera: camera.clone(),
                options: options.clone(),
                frame_origin,
                viewport: Viewport::new(internal_width, internal_height, depth_min, depth_max),
                camera_bind_group,
                _camera_buffer: camera_buffer,
                targets,
                statistics: RenderStatistics {
                    width: options.width,
                    height: options.height,
                    supersample: options.supersample,
                    internal_width,
                    internal_height,
                    backend_used: crate::RenderBackend::Gpu,
                    ..Default::default()
                },
                translucent: TranslucentStore::new(depth_min, depth_max),
            });
        }
        timings.wboit_used = gpu_views.iter().any(|view| view.targets.wboit.is_some());
        timings.setup_us = elapsed_us(setup_started);

        if let Some(message) = backend::take_error(&context.errors) {
            atlas::clear_cache();
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "GPU setup failed: {message}"
            ))));
        }

        Ok(Self {
            context,
            atlas,
            views: gpu_views,
            width: internal_width,
            height: internal_height,
            row_stride: row_stride as u32,
            readback_bytes,
            timings,
            total_started,
            staging: StagingRing::new(),
            instances: Vec::new(),
            translucent_scratch: Vec::new(),
            draws_submitted: 0,
        })
    }

    /// Encode and draw one contiguous page chunk into every view.
    pub fn render_pages(&mut self, pages: &[MeshPage]) -> Result<(), RenderError> {
        self.render_pages_inner(pages)
            .map_err(GpuFailure::into_error)
    }

    pub(super) fn render_pages_inner(&mut self, pages: &[MeshPage]) -> Result<(), GpuFailure> {
        let device = &self.context.device;
        let queue = &self.context.queue;

        let encode_started = Instant::now();
        self.instances.clear();
        let single_view = self.views.len() == 1;
        let first = self.views.first().expect("session has a view");
        let forward = single_view.then(|| first.camera.forward());
        let first_options = first.options.clone();
        let first_origin = first.frame_origin;
        for page in pages {
            let culled = instance::encode_page(
                page,
                forward,
                &first_options,
                first_origin,
                &mut self.instances,
            );
            if single_view {
                self.views[0].statistics.culled_backfaces += culled;
            }
        }
        if !single_view {
            for view in &mut self.views {
                for quad in &self.instances {
                    if quad.is_backface(view.camera.forward()) {
                        view.statistics.culled_backfaces += 1;
                    }
                }
            }
        }
        self.timings.encode_us += elapsed_us(encode_started);

        let project_started = Instant::now();
        let mut scratch = std::mem::take(&mut self.translucent_scratch);
        for view in &mut self.views {
            scratch.clear();
            for page in pages {
                push_page_translucent(
                    page,
                    &view.camera,
                    &view.options,
                    &mut view.statistics,
                    &mut scratch,
                );
            }
            if let Err(error) = view.translucent.push_many(std::mem::take(&mut scratch)) {
                self.translucent_scratch = scratch;
                return Err(GpuFailure::Failed(RenderError::temp_storage(format!(
                    "GPU translucent spill: {error}"
                ))));
            }
        }
        self.translucent_scratch = scratch;
        self.timings.project_us += elapsed_us(project_started);

        if self.instances.is_empty() {
            return Ok(());
        }

        for view in &mut self.views {
            for quad in &self.instances {
                if quad.is_backface(view.camera.forward()) {
                    continue;
                }
                view.statistics.triangles += 2;
                match quad.layer() {
                    1 => view.statistics.cutout_triangles += 2,
                    _ => view.statistics.opaque_triangles += 2,
                }
                if quad.texture != NO_TEXTURE {
                    view.statistics.textured_triangles += 2;
                }
            }
        }

        let mut upload_us = 0u64;
        let mut draw_us = 0u64;
        for batch in self.instances.chunks(MAX_INSTANCE_BYTES) {
            let upload_started = Instant::now();
            let buffer = match self.staging.upload(device, queue, batch) {
                Ok(buffer) => buffer,
                Err(failure) => return Err(self.classify(failure)),
            };
            upload_us += elapsed_us(upload_started);
            if let Some(message) = backend::take_error(&self.context.errors) {
                atlas::clear_cache();
                return Err(self.classify(GpuFailure::Failed(RenderError::gpu(format!(
                    "GPU instance upload failed: {message}"
                )))));
            }

            let draw_started = Instant::now();
            for view in &mut self.views {
                backend::draw_instances(
                    device,
                    queue,
                    &view.targets.color_view,
                    &view.targets.depth_view,
                    &self.context.instance_pipeline,
                    self.atlas.bind_group(view.options.texture_filter),
                    &view.camera_bind_group,
                    &buffer,
                    batch.len(),
                );
                self.draws_submitted += 1;
                if let Some(message) = backend::take_error(&self.context.errors) {
                    atlas::clear_cache();
                    return Err(GpuFailure::Failed(RenderError::gpu(format!(
                        "GPU instance draw failed: {message}"
                    ))));
                }
            }
            draw_us += elapsed_us(draw_started);
        }
        self.timings.upload_us += upload_us;
        self.timings.draw_us += draw_us;
        Ok(())
    }

    /// A failure before the first submitted draw is a capability/allocation
    /// failure, so `auto` may still fall back. After a draw it is fatal because
    /// the render target already contains partial output.
    fn classify(&self, failure: GpuFailure) -> GpuFailure {
        match failure {
            GpuFailure::Failed(error) if self.draws_submitted == 0 => {
                GpuFailure::Unavailable(error)
            }
            other => other,
        }
    }

    pub(super) fn finish_inner(self) -> Result<RenderOutput, GpuFailure> {
        let mut outputs = self.finish_views_inner()?;
        debug_assert_eq!(outputs.len(), 1);
        Ok(outputs.remove(0))
    }

    /// Finish a single-view session.
    pub fn finish(self) -> Result<RenderOutput, RenderError> {
        self.finish_inner().map_err(GpuFailure::into_error)
    }

    /// Draw translucent geometry, read back, and resolve every view in order.
    pub fn finish_views(self) -> Result<Vec<RenderOutput>, RenderError> {
        self.finish_views_inner().map_err(GpuFailure::into_error)
    }

    fn finish_views_inner(mut self) -> Result<Vec<RenderOutput>, GpuFailure> {
        let device = &self.context.device;
        let queue = &self.context.queue;
        let mut outputs = Vec::with_capacity(self.views.len());

        for view in &mut self.views {
            let translucent_started = Instant::now();
            if let Some(wboit) = view.targets.wboit.as_ref() {
                backend::clear_wboit_targets(device, queue, &wboit.accum_view, &wboit.reveal_view);
            }
            let parts = match view.translucent.finish() {
                Ok(parts) => parts,
                Err(error) => {
                    return Err(draw::temp_storage_failure(error));
                }
            };
            draw::draw_parts(
                &self.context,
                &self.atlas,
                &view.targets.color_view,
                &view.targets.depth_view,
                view.targets.wboit.as_ref(),
                &view.viewport,
                view.options.texture_filter,
                parts,
            )?;
            if let Some(wboit) = view.targets.wboit.as_ref() {
                backend::composite_wboit(
                    device,
                    queue,
                    &view.targets.color_view,
                    &self.context.wboit_composite_pipeline,
                    &wboit.bind_group,
                );
            }
            self.timings.translucent_us += elapsed_us(translucent_started);
            if let Some(message) = backend::take_error(&self.context.errors) {
                atlas::clear_cache();
                return Err(GpuFailure::Failed(RenderError::gpu(format!(
                    "GPU translucent draw failed: {message}"
                ))));
            }

            let readback_started = Instant::now();
            let readback = match backend::read_back(
                device,
                queue,
                &view.targets.color,
                self.width,
                self.height,
                self.row_stride,
                self.readback_bytes,
            ) {
                Ok(readback) => readback,
                Err(error) => {
                    atlas::clear_cache();
                    return Err(error);
                }
            };
            self.timings.readback_us += elapsed_us(readback_started);

            let resolve_started = Instant::now();
            let rgba = resolve::resolve(
                &readback,
                self.row_stride as usize,
                self.width,
                view.options.width,
                view.options.height,
                view.options.supersample,
                worker_count(view.options.threads, view.options.height as usize),
            );
            self.timings.resolve_us += elapsed_us(resolve_started);

            outputs.push(RenderOutput {
                width: view.options.width,
                height: view.options.height,
                rgba,
                statistics: view.statistics.clone(),
            });
        }

        self.timings.total_us = elapsed_us(self.total_started);
        for output in &mut outputs {
            output.statistics.gpu = self.timings.clone();
        }
        Ok(outputs)
    }
}

struct StagingRing {
    buffers: Vec<wgpu::Buffer>,
    capacity: u64,
    next: usize,
}

impl StagingRing {
    fn new() -> Self {
        Self {
            buffers: Vec::new(),
            capacity: 0,
            next: 0,
        }
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        quads: &[instance::GpuQuad],
    ) -> Result<wgpu::Buffer, GpuFailure> {
        let bytes = std::mem::size_of_val(quads) as u64;
        if bytes > device.limits().max_buffer_size {
            return Err(GpuFailure::Failed(RenderError::gpu(format!(
                "instance batch needs {bytes} bytes, GPU buffer limit is {}",
                device.limits().max_buffer_size
            ))));
        }

        if self.buffers.is_empty() || self.capacity < bytes {
            self.capacity = bytes.max(MIN_STAGING_BYTES).next_power_of_two();
            self.buffers.clear();
            for _ in 0..STAGING_SLOTS {
                self.buffers
                    .push(device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("blockscope gpu staging"),
                        size: self.capacity,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }));
            }
            self.next = 0;
        }

        let buffer = self.buffers[self.next].clone();
        self.next = (self.next + 1) % self.buffers.len();
        queue.write_buffer(&buffer, 0, bytemuck::cast_slice(quads));
        Ok(buffer)
    }
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
