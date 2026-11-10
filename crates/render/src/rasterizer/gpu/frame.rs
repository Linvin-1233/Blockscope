use super::super::bin::worker_count;
use super::super::geometry::{bounds, dot3};
use super::super::project::Triangle;
use super::super::{GpuStageTimings, RenderStatistics};
use super::translucent::TranslucentStore;
use super::vertex::Viewport;
use super::{GpuFailure, atlas, backend, draw, instance, resolve, targets};
use crate::{Camera, LightingProfile, RenderError, RenderOptions, RenderOutput};
use blockscope_material::TextureSet;
use blockscope_mesh::{Mesh, NO_TEXTURE, RenderLayer};
use blockscope_scene::{Bounds, Scene};
use std::sync::Arc;
use std::time::Instant;
use wgpu::util::DeviceExt;

const MAX_INSTANCE_BYTES: usize =
    backend::MAX_INSTANCE_BATCH_BYTES as usize / std::mem::size_of::<instance::GpuQuad>();

struct ResidentBatch {
    buffer: wgpu::Buffer,
    count: u32,
}

/// Static translucent data copied out of the mesh pages once. Re-projecting a
/// video frame then skips opaque quads and repeated page decoding.
struct ResidentTranslucent {
    quads: Vec<TranslucentQuad>,
}

struct TranslucentQuad {
    positions: [[f64; 3]; 4],
    normal: [f32; 3],
    color: [u8; 4],
    shade: f32,
    ao: [u8; 4],
    uvs: [[i16; 2]; 4],
    texture: u16,
}

impl ResidentTranslucent {
    fn build(mesh: &Mesh, lighting: LightingProfile) -> Self {
        let mut quads = Vec::new();
        for page in &mesh.pages {
            for quad in &page.quads {
                if quad.layer != RenderLayer::Translucent {
                    continue;
                }
                let normal = page.quad_normal(quad);
                let local = page.quad_positions(quad);
                let mut positions = [[0.0f64; 3]; 4];
                for (index, position) in local.iter().enumerate() {
                    positions[index] = [
                        f64::from(page.origin[0]) + f64::from(position[0]),
                        f64::from(page.origin[1]) + f64::from(position[1]),
                        f64::from(page.origin[2]) + f64::from(position[2]),
                    ];
                }
                let shade = if quad.shade {
                    lighting.face_shade(normal)
                } else {
                    1.0
                };
                let ao = match lighting {
                    LightingProfile::FlatV1 => [3u8; 4],
                    LightingProfile::SimpleAoV1 => quad.ao,
                };
                quads.push(TranslucentQuad {
                    positions,
                    normal,
                    color: quad.color,
                    shade,
                    ao,
                    uvs: quad.uvs(),
                    texture: quad.texture,
                });
            }
        }
        Self { quads }
    }

    fn project(
        &self,
        camera: &Camera,
        statistics: &mut RenderStatistics,
        translucent: &mut Vec<Triangle>,
    ) {
        for quad in &self.quads {
            if dot3(
                [
                    f64::from(quad.normal[0]),
                    f64::from(quad.normal[1]),
                    f64::from(quad.normal[2]),
                ],
                camera.forward(),
            ) >= 0.0
            {
                statistics.culled_backfaces += 1;
                continue;
            }

            let mut projected = [[0.0f64; 3]; 4];
            for (index, position) in quad.positions.iter().enumerate() {
                projected[index] = camera.project(*position);
            }

            for indices in [[0usize, 1, 2], [0, 2, 3]] {
                let points = [
                    projected[indices[0]],
                    projected[indices[1]],
                    projected[indices[2]],
                ];
                let bbox = bounds(&points);
                if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
                    continue;
                }

                statistics.translucent_triangles += 1;
                if quad.texture != NO_TEXTURE {
                    statistics.textured_triangles += 1;
                }
                statistics.triangles += 1;
                let depth_key = (points[0][2] + points[1][2] + points[2][2]) / 3.0;
                translucent.push(Triangle {
                    points: points.map(|point| point.map(|value| value as f32)),
                    color: quad.color,
                    shade: quad.shade,
                    ao: [
                        quad.ao[indices[0]],
                        quad.ao[indices[1]],
                        quad.ao[indices[2]],
                    ],
                    uvs: [
                        quad.uvs[indices[0]],
                        quad.uvs[indices[1]],
                        quad.uvs[indices[2]],
                    ],
                    texture: quad.texture,
                    depth_key,
                });
            }
        }
    }
}

/// Reusable GPU session for a fixed mesh and a sequence of cameras.
///
/// Opaque and cutout quads are encoded into resident vertex buffers once, so a
/// video frame loop only rewrites the camera uniform and re-projects the
/// translucent layer. This is the session shape the video renderer needs; the
/// resident geometry must fit the adapter working set, so large scenes still
/// use the streaming [`super::GpuRenderSession`] path.
pub struct GpuFrameSession<'a> {
    context: Arc<backend::GpuContext>,
    atlas: atlas::CachedAtlas,
    _mesh: &'a Mesh,
    bounds: Bounds,
    base_options: RenderOptions,
    width: u32,
    height: u32,
    row_stride: u32,
    readback_buffer: wgpu::Buffer,
    frame_origin: [i32; 3],
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    targets: targets::RenderTargets,
    translucent: ResidentTranslucent,
    batches: Vec<ResidentBatch>,
    resident_quads: u64,
    resident_bytes: u64,
    base_statistics: RenderStatistics,
    setup_timings: GpuStageTimings,
    frames_rendered: u64,
}

impl<'a> GpuFrameSession<'a> {
    /// Upload the opaque/cutout mesh once. `options` supplies the output size,
    /// lighting profile, background, and initial camera framing.
    pub fn new(
        scene: &Scene,
        mesh: &'a Mesh,
        options: &RenderOptions,
        textures: Option<&TextureSet>,
    ) -> Result<Self, RenderError> {
        Self::new_inner(scene, mesh, options, textures).map_err(GpuFailure::into_error)
    }

    fn new_inner(
        scene: &Scene,
        mesh: &'a Mesh,
        options: &RenderOptions,
        textures: Option<&TextureSet>,
    ) -> Result<Self, GpuFailure> {
        let setup_started = Instant::now();
        options.validate().map_err(GpuFailure::Unavailable)?;
        let camera = Camera::new(&scene.bounds, options).map_err(GpuFailure::Unavailable)?;
        let context = backend::shared_context()?;
        let device = &context.device;
        let queue = &context.queue;
        let _ = backend::take_error(&context.errors);

        let width = options.internal_width();
        let height = options.internal_height();
        let limits = device.limits();
        if width > limits.max_texture_dimension_2d || height > limits.max_texture_dimension_2d {
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "output {width}x{height} exceeds the GPU texture limit {}",
                limits.max_texture_dimension_2d
            ))));
        }
        let row_stride = backend::align_to(
            width as usize * 4,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize,
        );
        let readback_bytes = row_stride as u64 * u64::from(height);
        if readback_bytes > limits.max_buffer_size {
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "readback needs {readback_bytes} bytes, GPU buffer limit is {}",
                limits.max_buffer_size
            ))));
        }

        let readback_buffer = backend::create_readback_buffer(device, readback_bytes);
        let (gpu_camera, frame_origin) =
            super::camera_uniform(scene, &camera).map_err(GpuFailure::Unavailable)?;
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blockscope gpu frame camera"),
            contents: bytemuck::bytes_of(&gpu_camera),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blockscope gpu frame camera bind group"),
            layout: &context.camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

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
        let atlas_us = elapsed_us(atlas_started);

        let resident_bytes = mesh
            .statistics
            .opaque_quads
            .saturating_add(mesh.statistics.cutout_quads)
            .saturating_mul(std::mem::size_of::<instance::GpuQuad>() as u64);
        let targets_bytes = super::budget::estimate_session_bytes(
            width,
            height,
            1,
            context.wboit_supported,
            0,
            0,
            atlas.bytes,
        );
        let required = targets_bytes.saturating_add(resident_bytes);
        if required > context.working_set_bytes {
            return Err(GpuFailure::Unavailable(RenderError::unavailable(format!(
                "resident frame session needs {required} bytes, 60% VRAM budget is {}",
                context.working_set_bytes
            ))));
        }

        let targets = targets::create(&context, width, height, options.background)?;
        let translucent = ResidentTranslucent::build(mesh, options.lighting);

        let encode_started = Instant::now();
        let mut batches = Vec::new();
        let mut scratch: Vec<instance::GpuQuad> = Vec::new();
        let mut resident_quads = 0u64;
        let mut statistics = RenderStatistics {
            width: options.width,
            height: options.height,
            supersample: options.supersample,
            internal_width: width,
            internal_height: height,
            backend_used: crate::RenderBackend::Gpu,
            ..Default::default()
        };
        for page in &mesh.pages {
            instance::encode_page(page, None, options, frame_origin, &mut scratch);
            if scratch.len() >= MAX_INSTANCE_BYTES {
                upload_batch(device, queue, &mut scratch, &mut batches, &mut statistics)?;
                resident_quads += batches.last().map_or(0, |batch| u64::from(batch.count));
            }
        }
        if !scratch.is_empty() {
            upload_batch(device, queue, &mut scratch, &mut batches, &mut statistics)?;
            resident_quads += batches.last().map_or(0, |batch| u64::from(batch.count));
        }
        let encode_us = elapsed_us(encode_started);

        let mut setup_timings = GpuStageTimings {
            atlas_us,
            atlas_cache_hit: atlas.cache_hit,
            wboit_used: targets.wboit.is_some(),
            encode_us,
            ..Default::default()
        };
        setup_timings.setup_us = elapsed_us(setup_started);

        Ok(Self {
            context,
            atlas,
            _mesh: mesh,
            bounds: scene.bounds,
            base_options: options.clone(),
            width,
            height,
            row_stride: row_stride as u32,
            readback_buffer,
            frame_origin,
            camera_buffer,
            camera_bind_group,
            targets,
            translucent,
            batches,
            resident_quads,
            resident_bytes,
            base_statistics: statistics,
            setup_timings,
            frames_rendered: 0,
        })
    }

    /// Resident opaque/cutout instance bytes uploaded at construction.
    pub const fn resident_bytes(&self) -> u64 {
        self.resident_bytes
    }

    /// Number of opaque/cutout quads uploaded at construction.
    pub const fn resident_quads(&self) -> u64 {
        self.resident_quads
    }

    /// Render one camera frame from the resident mesh.
    pub fn render_frame(&mut self, options: &RenderOptions) -> Result<RenderOutput, RenderError> {
        self.render_frame_inner(options)
            .map_err(GpuFailure::into_error)
    }

    fn render_frame_inner(&mut self, options: &RenderOptions) -> Result<RenderOutput, GpuFailure> {
        options.validate().map_err(GpuFailure::Unavailable)?;
        if options.internal_width() != self.width || options.internal_height() != self.height {
            return Err(GpuFailure::Unavailable(RenderError::invalid(
                "frame size must match the resident session",
            )));
        }
        if options.supersample != self.base_options.supersample {
            return Err(GpuFailure::Unavailable(RenderError::invalid(
                "frame supersample must match the resident session",
            )));
        }
        if options.lighting != self.base_options.lighting {
            return Err(GpuFailure::Unavailable(RenderError::invalid(
                "frame lighting must match the resident session",
            )));
        }

        let frame_started = Instant::now();
        let camera = Camera::new(&self.bounds, options).map_err(GpuFailure::Unavailable)?;
        let (gpu_camera, origin) = super::camera_uniform_for_bounds(&self.bounds, &camera)
            .map_err(GpuFailure::Unavailable)?;
        if origin != self.frame_origin {
            return Err(GpuFailure::Unavailable(RenderError::invalid(
                "frame camera origin changed; the resident mesh cannot be reused",
            )));
        }
        self.context
            .queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&gpu_camera));

        let device = &self.context.device;
        let queue = &self.context.queue;
        backend::clear_targets(
            device,
            queue,
            &self.targets.color_view,
            &self.targets.depth_view,
            options.background,
        );
        if let Some(wboit) = self.targets.wboit.as_ref() {
            backend::clear_wboit_targets(device, queue, &wboit.accum_view, &wboit.reveal_view);
        }

        let (depth_min, depth_max) = super::depth_range_bounds(&self.bounds, &camera);
        let viewport = Viewport::new(self.width, self.height, depth_min, depth_max);

        let draw_started = Instant::now();
        for batch in &self.batches {
            backend::draw_instances(
                device,
                queue,
                &self.targets.color_view,
                &self.targets.depth_view,
                &self.context.instance_pipeline,
                self.atlas.bind_group(options.texture_filter),
                &self.camera_bind_group,
                &batch.buffer,
                batch.count as usize,
            );
        }
        let draw_us = elapsed_us(draw_started);
        if let Some(message) = backend::take_error(&self.context.errors) {
            atlas::clear_cache();
            return Err(GpuFailure::Failed(RenderError::gpu(format!(
                "GPU resident draw failed: {message}"
            ))));
        }

        let mut statistics = self.base_statistics.clone();
        let project_started = Instant::now();
        let mut translucent = Vec::new();
        self.translucent
            .project(&camera, &mut statistics, &mut translucent);
        let mut store = TranslucentStore::new(depth_min, depth_max);
        if let Err(error) = store.push_many(translucent) {
            return Err(draw::temp_storage_failure(error));
        }
        let parts = match store.finish() {
            Ok(parts) => parts,
            Err(error) => return Err(draw::temp_storage_failure(error)),
        };
        let project_us = elapsed_us(project_started);

        let translucent_started = Instant::now();
        draw::draw_parts(
            &self.context,
            &self.atlas,
            &self.targets.color_view,
            &self.targets.depth_view,
            self.targets.wboit.as_ref(),
            &viewport,
            options.texture_filter,
            parts,
        )?;
        if let Some(wboit) = self.targets.wboit.as_ref() {
            backend::composite_wboit(
                device,
                queue,
                &self.targets.color_view,
                &self.context.wboit_composite_pipeline,
                &wboit.bind_group,
            );
        }
        let translucent_us = elapsed_us(translucent_started);
        if let Some(message) = backend::take_error(&self.context.errors) {
            atlas::clear_cache();
            return Err(GpuFailure::Failed(RenderError::gpu(format!(
                "GPU translucent draw failed: {message}"
            ))));
        }

        let readback_started = Instant::now();
        let mut resolve_us = 0;
        let rgba = backend::read_back(
            device,
            queue,
            &self.targets.color,
            &self.readback_buffer,
            self.width,
            self.height,
            self.row_stride,
            |readback| {
                let resolve_started = Instant::now();
                let rgba = resolve::resolve(
                    readback,
                    self.row_stride as usize,
                    self.width,
                    options.width,
                    options.height,
                    options.supersample,
                    worker_count(options.threads, options.height as usize),
                );
                resolve_us = elapsed_us(resolve_started);
                rgba
            },
        )?;
        let readback_us = elapsed_us(readback_started).saturating_sub(resolve_us);

        let mut timings = if self.frames_rendered == 0 {
            self.setup_timings.clone()
        } else {
            GpuStageTimings {
                wboit_used: self.setup_timings.wboit_used,
                ..Default::default()
            }
        };
        timings.project_us = project_us;
        timings.draw_us = draw_us;
        timings.translucent_us = translucent_us;
        timings.readback_us = readback_us;
        timings.resolve_us = resolve_us;
        timings.total_us = elapsed_us(frame_started);
        self.frames_rendered += 1;
        statistics.gpu = timings;

        Ok(RenderOutput {
            width: options.width,
            height: options.height,
            rgba,
            statistics,
        })
    }

    /// Render a sequence of cameras in order. This is the frame loop the video
    /// encoder consumes; each frame reuses the resident opaque/cutout buffers.
    pub fn render_frames<F>(
        &mut self,
        frames: &[RenderOptions],
        mut sink: F,
    ) -> Result<(), RenderError>
    where
        F: FnMut(usize, RenderOutput) -> Result<(), RenderError>,
    {
        for (index, frame) in frames.iter().enumerate() {
            let output = self.render_frame(frame)?;
            sink(index, output)?;
        }
        Ok(())
    }
}

fn upload_batch(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scratch: &mut Vec<instance::GpuQuad>,
    batches: &mut Vec<ResidentBatch>,
    statistics: &mut RenderStatistics,
) -> Result<(), GpuFailure> {
    if scratch.is_empty() {
        return Ok(());
    }
    for quad in scratch.iter() {
        statistics.triangles += 2;
        match quad.layer() {
            1 => statistics.cutout_triangles += 2,
            _ => statistics.opaque_triangles += 2,
        }
        if quad.texture != NO_TEXTURE {
            statistics.textured_triangles += 2;
        }
    }
    let bytes: &[u8] = bytemuck::cast_slice(scratch);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("blockscope gpu resident instances"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, bytes);
    batches.push(ResidentBatch {
        buffer,
        count: u32::try_from(scratch.len()).map_err(|_| {
            GpuFailure::Failed(RenderError::gpu("resident batch exceeds u32 instances"))
        })?,
    });
    scratch.clear();
    Ok(())
}

fn elapsed_us(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}
