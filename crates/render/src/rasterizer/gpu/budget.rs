use wgpu::DeviceType;

/// Fraction of adapter VRAM available to this process. Render targets, chunk
/// staging, readback, and the atlas cache all share it with other applications,
/// so the engine keeps a fixed headroom instead of filling the card.
pub(super) const WORKING_SET_PERCENT: u64 = 60;

/// Conservative lower bounds when the platform cannot report VRAM. Discrete
/// cards are assumed to have at least 2 GiB and integrated/virtual adapters at
/// least 1 GiB of usable GPU memory. An over-estimate is safe because a failed
/// pre-draw allocation returns `BACKEND_UNAVAILABLE` and `auto` falls back.
const DISCRETE_FLOOR_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const INTEGRATED_FLOOR_BYTES: u64 = 1024 * 1024 * 1024;
const OTHER_FLOOR_BYTES: u64 = 1024 * 1024 * 1024;

/// Atlas cache share of the working set. The rest is left for targets, staging,
/// readback, and next-frame headroom.
const ATLAS_CACHE_DIVISOR: u64 = 4;
const MIN_ATLAS_CACHE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ATLAS_CACHE_BYTES: u64 = 512 * 1024 * 1024;

/// The application-level override used by deployments and tests on platforms
/// without a VRAM query.
const VRAM_OVERRIDE: &str = "BLOCKSCOPE_GPU_VRAM_BYTES";

pub(super) fn working_set_bytes(vram_bytes: u64) -> u64 {
    vram_bytes.saturating_mul(WORKING_SET_PERCENT) / 100
}

/// Best-effort VRAM lower bound for an adapter.
///
/// Linux exposes the value through sysfs for several drivers. Other platforms
/// use a conservative floor; `BLOCKSCOPE_GPU_VRAM_BYTES` overrides the probe
/// when an operator knows the real value.
pub(super) fn detect_vram_bytes(device_type: DeviceType) -> u64 {
    if let Some(value) = std::env::var_os(VRAM_OVERRIDE)
        && let Some(value) = value.to_str()
        && let Ok(bytes) = value.trim().parse::<u64>()
        && bytes > 0
    {
        return bytes;
    }

    #[cfg(target_os = "linux")]
    if let Some(bytes) = linux_vram_bytes() {
        return bytes;
    }

    match device_type {
        DeviceType::DiscreteGpu => DISCRETE_FLOOR_BYTES,
        DeviceType::IntegratedGpu | DeviceType::VirtualGpu => INTEGRATED_FLOOR_BYTES,
        _ => OTHER_FLOOR_BYTES,
    }
}

pub(super) fn atlas_cache_budget(working_set_bytes: u64) -> u64 {
    if working_set_bytes == 0 {
        return MIN_ATLAS_CACHE_BYTES;
    }
    (working_set_bytes / ATLAS_CACHE_DIVISOR).clamp(MIN_ATLAS_CACHE_BYTES, MAX_ATLAS_CACHE_BYTES)
}

/// Bytes retained by one session: per-view color/depth/WBOIT targets, one
/// readback buffer and pixel copy per view, three staging slots, and the atlas.
pub(super) fn estimate_session_bytes(
    width: u32,
    height: u32,
    views: usize,
    wboit: bool,
    max_instance_batch_bytes: u64,
    staging_slots: u64,
    atlas_bytes: u64,
) -> u64 {
    let pixels = u64::from(width) * u64::from(height);
    let row_stride = align_to(
        width as usize * 4,
        wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize,
    ) as u64;
    let readback = row_stride * u64::from(height);

    // Rgba8 color + Depth32Float depth, plus two Rgba16Float WBOIT targets.
    let targets = if wboit {
        pixels * (4 + 4 + 16)
    } else {
        pixels * (4 + 4)
    };
    let per_view = targets + readback;
    per_view
        .saturating_mul(views.max(1) as u64)
        .saturating_add(max_instance_batch_bytes.saturating_mul(staging_slots))
        .saturating_add(atlas_bytes)
}

fn align_to(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

#[cfg(target_os = "linux")]
fn linux_vram_bytes() -> Option<u64> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    let mut best = 0u64;
    for entry in entries.flatten() {
        let path = entry.path().join("device/mem_info_vram_total");
        let Ok(value) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(bytes) = value.trim().parse::<u64>() else {
            continue;
        };
        best = best.max(bytes);
    }
    (best > 0).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_set_is_60_percent() {
        assert_eq!(working_set_bytes(4 * 1024 * 1024 * 1024), 2_576_980_377);
        assert_eq!(working_set_bytes(0), 0);
    }

    #[test]
    fn atlas_budget_is_bounded() {
        assert_eq!(atlas_cache_budget(0), MIN_ATLAS_CACHE_BYTES);
        assert_eq!(
            atlas_cache_budget(10 * 1024 * 1024 * 1024),
            MAX_ATLAS_CACHE_BYTES
        );
    }

    #[test]
    fn session_estimate_scales_with_views_and_wboit() {
        let one = estimate_session_bytes(2048, 2048, 1, true, 32 * 1024 * 1024, 3, 0);
        let two = estimate_session_bytes(2048, 2048, 2, true, 32 * 1024 * 1024, 3, 0);
        assert!(two > one);
        assert_eq!(two - one, one - 3 * 32 * 1024 * 1024);

        let without = estimate_session_bytes(2048, 2048, 1, false, 32 * 1024 * 1024, 3, 0);
        assert_eq!(one - without, 2048u64 * 2048 * 16);
    }

    #[test]
    fn session_estimate_includes_alignment_and_atlas() {
        // 1024 px wide is already 4096 bytes, a multiple of the 256-byte
        // copy-row alignment; 1000 px is not.
        let aligned = estimate_session_bytes(1000, 10, 1, false, 0, 0, 0);
        let row_stride = align_to(1000 * 4, 256) as u64;
        assert_eq!(aligned, 1000u64 * 10 * 8 + row_stride * 10);
        assert_eq!(
            estimate_session_bytes(1000, 10, 1, false, 0, 0, 1234),
            aligned + 1234
        );
    }
}
