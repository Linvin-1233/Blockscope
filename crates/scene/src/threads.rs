/// Hard cap for worker threads in a single build/render stage.
pub const MAX_WORKERS: usize = 256;

/// Resolve a requested worker count against the amount of available work.
///
/// `0` selects the available parallelism. The result is always at least one and
/// never exceeds [`MAX_WORKERS`] or the number of independent jobs.
pub fn worker_count(requested: u32, jobs: usize) -> usize {
    let available = if requested == 0 {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    } else {
        requested as usize
    };

    available.clamp(1, MAX_WORKERS.min(jobs.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_count_is_capped_and_always_positive() {
        assert_eq!(worker_count(u32::MAX, usize::MAX), MAX_WORKERS);
        assert_eq!(worker_count(1, 100), 1);
        assert_eq!(worker_count(0, 1), 1);
        assert!(worker_count(0, usize::MAX) <= MAX_WORKERS);
    }
}
