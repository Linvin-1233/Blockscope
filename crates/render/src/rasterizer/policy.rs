use crate::{RenderBackend, RenderOptions};
use litematic_scene::Scene;

/// Whether a hardware adapter can be used for GPU rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuAdapterKind {
    /// The GPU feature is compiled out or no adapter was found.
    None,
    /// Only a software adapter (lavapipe, WARP, and similar) is available.
    Software,
    /// A hardware adapter is available.
    Hardware,
}

/// Deterministic machine and scene description for [`decide_backend`].
///
/// The policy deliberately takes plain values instead of reading the host
/// directly, so the same policy can be unit-tested without a GPU and produces
/// the same decision for the same description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendPolicyInput {
    /// The `gpu` feature is compiled into this build.
    pub gpu_compiled: bool,
    /// Adapter class reported by the GPU capability probe.
    pub adapter: GpuAdapterKind,
    /// Camera-relative f32 coordinates are safe for this scene span.
    pub safe_precision: bool,
    /// Resolved worker thread count. Zero is not accepted here because it would
    /// make the decision depend on the host instead of the description.
    pub threads: u32,
    /// Quad count from the mesh, or a deterministic estimate for streaming.
    pub quads: u64,
    /// Internal (supersampled) pixel count.
    pub internal_pixels: u64,
    /// The caller selected the streaming renderer.
    pub streaming: bool,
    /// Number of views sharing one GPU session. Setup is paid once.
    pub views: u32,
    /// The process already paid GPU device and pipeline setup.
    pub gpu_initialized: bool,
    /// 60 percent of the adapter VRAM, or zero when unknown.
    pub working_set_bytes: u64,
    /// Estimated GPU bytes for targets, readback, staging, and the atlas.
    pub required_bytes: u64,
}

/// Chosen backend plus the policy reason that selected it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendDecision {
    pub backend: RenderBackend,
    pub reason: &'static str,
}

impl BackendDecision {
    const fn cpu(reason: &'static str) -> Self {
        Self {
            backend: RenderBackend::Cpu,
            reason,
        }
    }

    const fn gpu(reason: &'static str) -> Self {
        Self {
            backend: RenderBackend::Gpu,
            reason,
        }
    }
}

/// Below this size device setup and readback dominate any rasterizer win.
const MIN_GPU_QUADS: u64 = 10_000;

/// On an uninitialized context a strong CPU is preferred. Shader compilation
/// and device setup cost hundreds of milliseconds, which one-shot CLI renders
/// cannot amortize; a warm process may still choose the GPU.
const STRONG_CPU_THREADS: u32 = 8;

/// CPU raster cost per quad on the 12-thread reference host, in microseconds.
const CPU_QUAD_US_12T: f64 = 0.085;
/// CPU raster cost per internal pixel on the reference host, in microseconds.
const CPU_PIXEL_US_12T: f64 = 0.0025;
/// GPU device/target setup, shader compilation, and readback cost when the
/// process-wide context does not exist yet.
const GPU_FIXED_US: f64 = 200_000.0;
/// GPU setup cost after the process-wide context already exists.
const GPU_FIXED_US_INITIALIZED: f64 = 3_000.0;
/// GPU instance encode, upload, and draw cost per quad, in microseconds.
const GPU_QUAD_US: f64 = 0.045;
/// GPU raster cost per internal pixel, in microseconds.
const GPU_PIXEL_US: f64 = 0.000_1;
/// Per-chunk staging and spill overhead of the streaming path.
const STREAMING_GPU_OVERHEAD: f64 = 1.10;
/// Require a predicted 5 percent win before paying the approximation cost.
const GPU_WIN_MARGIN: f64 = 0.95;

/// Select CPU or GPU from a machine and scene description.
///
/// The cost model is calibrated on the reference host and scales the CPU term
/// by `(12 / per_view_threads)^0.35`, which matches the measured 12-thread and
/// 2-thread runs without pretending that rasterization scales linearly with
/// cores. Multi-view jobs pay GPU setup once and split the CPU thread budget
/// across views.
pub fn decide_backend(input: &BackendPolicyInput) -> BackendDecision {
    if !input.gpu_compiled {
        return BackendDecision::cpu("gpu-feature-disabled");
    }
    match input.adapter {
        GpuAdapterKind::None => return BackendDecision::cpu("no-hardware-adapter"),
        GpuAdapterKind::Software => return BackendDecision::cpu("software-adapter"),
        GpuAdapterKind::Hardware => {}
    }
    if !input.safe_precision {
        return BackendDecision::cpu("unsafe-f32-precision");
    }
    if input.working_set_bytes == 0 || input.required_bytes > input.working_set_bytes {
        return BackendDecision::cpu("gpu-memory-budget-exceeded");
    }
    if input.quads < MIN_GPU_QUADS {
        return BackendDecision::cpu("mesh-too-small");
    }
    if !input.gpu_initialized && input.threads >= STRONG_CPU_THREADS && input.views <= 1 {
        return BackendDecision::cpu("strong-cpu-setup-not-amortized");
    }

    let views = input.views.max(1);
    // Views are rendered with the thread budget split across the batch, so a
    // multi-view job pays more CPU time per view but only one GPU setup.
    let per_view_threads = (input.threads / views).max(1) as f64;
    let cpu_scale = (12.0 / per_view_threads).powf(0.35);
    let cpu_per_view_us = (input.quads as f64 * CPU_QUAD_US_12T
        + input.internal_pixels as f64 * CPU_PIXEL_US_12T)
        * cpu_scale;
    let cpu_us = cpu_per_view_us * f64::from(views);
    let fixed_us = if input.gpu_initialized {
        GPU_FIXED_US_INITIALIZED
    } else {
        GPU_FIXED_US
    };
    let mut gpu_us = fixed_us
        + (input.quads as f64 * GPU_QUAD_US + input.internal_pixels as f64 * GPU_PIXEL_US)
            * f64::from(views);
    if input.streaming {
        gpu_us *= STREAMING_GPU_OVERHEAD;
    }

    if gpu_us >= cpu_us * GPU_WIN_MARGIN {
        BackendDecision::cpu("cpu-predicted-faster")
    } else {
        BackendDecision::gpu("gpu-predicted-faster")
    }
}

/// Resolve `Auto` against the current machine and scene.
///
/// Explicit `Cpu` and `Gpu` values are returned unchanged. `quads` is the real
/// mesh size when available, or the deterministic page sample used before a
/// streaming build. `texture_bytes` is the material snapshot texture size; it
/// is part of the GPU memory admission check.
pub fn select_backend(
    scene: &Scene,
    options: &RenderOptions,
    quads: u64,
    streaming: bool,
    texture_bytes: u64,
    views: usize,
) -> BackendDecision {
    if options.backend != RenderBackend::Auto {
        return BackendDecision {
            backend: options.backend,
            reason: "explicit-backend",
        };
    }

    #[cfg(feature = "gpu")]
    {
        // Avoid paying adapter probing for jobs that can never amortize it.
        if quads < MIN_GPU_QUADS {
            return BackendDecision::cpu("mesh-too-small");
        }
        let Some(adapter) = super::gpu::adapter_profile() else {
            return BackendDecision::cpu("no-hardware-adapter");
        };
        let required = super::gpu::estimate_session_bytes(
            options.internal_width(),
            options.internal_height(),
            1,
            adapter.wboit_supported,
            texture_bytes,
        );
        let threads = if options.threads == 0 {
            std::thread::available_parallelism()
                .map(|value| value.get() as u32)
                .unwrap_or(1)
        } else {
            options.threads
        };
        let input = BackendPolicyInput {
            gpu_compiled: true,
            adapter: if adapter.software {
                GpuAdapterKind::Software
            } else {
                GpuAdapterKind::Hardware
            },
            safe_precision: precision_is_safe(scene),
            threads,
            quads,
            internal_pixels: u64::from(options.internal_width())
                * u64::from(options.internal_height()),
            streaming,
            views: views.max(1) as u32,
            gpu_initialized: super::gpu::context_initialized(),
            working_set_bytes: adapter.working_set_bytes,
            required_bytes: required,
        };
        decide_backend(&input)
    }

    #[cfg(not(feature = "gpu"))]
    {
        let _ = (scene, quads, streaming, texture_bytes, views);
        BackendDecision::cpu("gpu-feature-disabled")
    }
}

#[cfg(feature = "gpu")]
fn precision_is_safe(scene: &Scene) -> bool {
    let min = scene.bounds.min;
    let max = scene.bounds.max_exclusive;
    (0..3).all(|axis| {
        let span = (max[axis] - min[axis]) as f64;
        span <= crate::camera::GPU_SAFE_SPAN
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> BackendPolicyInput {
        BackendPolicyInput {
            gpu_compiled: true,
            adapter: GpuAdapterKind::Hardware,
            safe_precision: true,
            threads: 12,
            quads: 2_300_000,
            internal_pixels: 4_194_304,
            streaming: false,
            views: 1,
            gpu_initialized: true,
            working_set_bytes: 2_400 * 1024 * 1024,
            required_bytes: 400 * 1024 * 1024,
        }
    }

    #[test]
    fn reference_000002_predicts_a_gpu_win() {
        let decision = decide_backend(&input());
        assert_eq!(decision.backend, RenderBackend::Gpu);
        assert_eq!(decision.reason, "gpu-predicted-faster");
    }

    #[test]
    fn weak_cpu_switches_to_the_gpu_sooner() {
        let mut tiny = input();
        tiny.quads = 2_300_000;
        tiny.internal_pixels = 4_194_304;
        tiny.gpu_initialized = false;
        assert_eq!(decide_backend(&tiny).backend, RenderBackend::Cpu);

        tiny.threads = 2;
        assert_eq!(decide_backend(&tiny).backend, RenderBackend::Gpu);
    }

    #[test]
    fn small_meshes_stay_on_the_cpu() {
        let mut tiny = input();
        tiny.quads = 1_000;
        assert_eq!(decide_backend(&tiny).backend, RenderBackend::Cpu);
        assert_eq!(decide_backend(&tiny).reason, "mesh-too-small");
    }

    #[test]
    fn missing_or_software_adapters_stay_on_the_cpu() {
        let mut missing = input();
        missing.adapter = GpuAdapterKind::None;
        assert_eq!(decide_backend(&missing).reason, "no-hardware-adapter");

        let mut software = input();
        software.adapter = GpuAdapterKind::Software;
        assert_eq!(decide_backend(&software).reason, "software-adapter");
    }

    #[test]
    fn unsafe_precision_stays_on_the_cpu() {
        let mut unsafe_input = input();
        unsafe_input.safe_precision = false;
        assert_eq!(decide_backend(&unsafe_input).reason, "unsafe-f32-precision");
    }

    #[test]
    fn over_budget_work_stays_on_the_cpu() {
        let mut over = input();
        over.required_bytes = over.working_set_bytes + 1;
        assert_eq!(decide_backend(&over).reason, "gpu-memory-budget-exceeded");
    }

    #[test]
    fn streaming_overhead_can_flip_a_marginal_win() {
        let mut marginal = input();
        marginal.quads = 1_750_000;
        marginal.internal_pixels = 4_194_304;
        marginal.threads = 2;
        marginal.gpu_initialized = false;
        assert_eq!(decide_backend(&marginal).backend, RenderBackend::Gpu);

        marginal.streaming = true;
        assert_eq!(decide_backend(&marginal).backend, RenderBackend::Cpu);
    }

    #[test]
    fn one_shot_strong_cpu_avoids_unamortized_setup() {
        let mut one_shot = input();
        one_shot.gpu_initialized = false;
        assert_eq!(decide_backend(&one_shot).backend, RenderBackend::Cpu);
        assert_eq!(
            decide_backend(&one_shot).reason,
            "strong-cpu-setup-not-amortized"
        );

        // A process that already paid setup can use the GPU.
        one_shot.gpu_initialized = true;
        assert_eq!(decide_backend(&one_shot).backend, RenderBackend::Gpu);
    }

    #[test]
    fn multi_view_amortizes_gpu_setup() {
        let mut multi = input();
        multi.gpu_initialized = false;
        multi.views = 1;
        assert_eq!(decide_backend(&multi).backend, RenderBackend::Cpu);

        multi.views = 2;
        assert_eq!(decide_backend(&multi).backend, RenderBackend::Gpu);
    }

    #[test]
    fn initialized_context_lowers_the_setup_term() {
        let mut marginal = input();
        marginal.quads = 80_000;
        marginal.internal_pixels = 1_048_576;
        marginal.gpu_initialized = false;
        assert_eq!(decide_backend(&marginal).backend, RenderBackend::Cpu);

        marginal.gpu_initialized = true;
        assert_eq!(decide_backend(&marginal).backend, RenderBackend::Gpu);
    }

    #[test]
    fn decision_is_deterministic_for_one_description() {
        let first = decide_backend(&input());
        for _ in 0..10 {
            assert_eq!(decide_backend(&input()), first);
        }
    }

    #[test]
    fn explicit_backends_are_not_policy_inputs() {
        let scene = Scene {
            metadata: Default::default(),
            bounds: litematic_scene::Bounds {
                min: [0, 0, 0],
                max_exclusive: [1, 1, 1],
            },
            regions: Vec::new(),
            statistics: Default::default(),
            diagnostics: Vec::new(),
        };
        for backend in [RenderBackend::Cpu, RenderBackend::Gpu] {
            let options = RenderOptions {
                backend,
                ..Default::default()
            };
            let decision = select_backend(&scene, &options, 1, false, 0, 1);
            assert_eq!(decision.backend, backend);
            assert_eq!(decision.reason, "explicit-backend");
        }
    }
}
