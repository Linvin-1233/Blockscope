use serde::Serialize;
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::PathBuf;

const MIB: u64 = 1024 * 1024;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const GIB: u64 = 1024 * MIB;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const UNLIMITED_FLOOR: u64 = 1 << 60;

/// Detected machine or container capacity plus suggested pool settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capacity {
    pub platform: String,
    pub cores: u32,
    /// CPU quota in milli-cores; `None` means unlimited or unknown.
    pub cpu_quota_milli: Option<u32>,
    pub memory_limit_bytes: u64,
    pub memory_available_bytes: u64,
    pub suggested_workers: u32,
    pub suggested_threads: u32,
    pub default_width: u32,
    pub default_height: u32,
    pub default_supersample: u32,
}

impl Capacity {
    pub fn detect() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|value| value.get() as u32)
            .unwrap_or(1);
        let limits = platform_limits();
        let plan = plan(cores, limits.memory_limit);

        Self {
            platform: std::env::consts::OS.to_string(),
            cores,
            cpu_quota_milli: limits.cpu_quota_milli,
            memory_limit_bytes: limits.memory_limit,
            memory_available_bytes: limits.memory_available,
            suggested_workers: plan.workers,
            suggested_threads: plan.threads,
            default_width: plan.width,
            default_height: plan.height,
            default_supersample: plan.supersample,
        }
    }

    pub fn tier(&self) -> String {
        format!(
            "{}x{} @ ss{}",
            self.default_width, self.default_height, self.default_supersample
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct Plan {
    workers: u32,
    threads: u32,
    width: u32,
    height: u32,
    supersample: u32,
}

/// Size the pool from cores and the memory ceiling.
///
/// One worker per two cores keeps a single job fast while letting two jobs
/// overlap; four workers is the cap because render throughput saturates there.
fn plan(cores: u32, memory_limit: u64) -> Plan {
    let (width, height, supersample) = default_tier(cores, memory_limit);
    let per_worker = match (width, supersample) {
        (2048, _) => 400 * MIB,
        (_, 2) => 300 * MIB,
        _ => 250 * MIB,
    };

    let workers_by_cpu = (cores / 2).clamp(1, 4);
    let workers_by_memory = if memory_limit == 0 {
        workers_by_cpu
    } else {
        let reserve = (memory_limit / 100 * 15).max(256 * MIB);
        let usable = memory_limit.saturating_sub(reserve);
        ((usable / per_worker) as u32).max(1)
    };

    let workers = workers_by_cpu.min(workers_by_memory).max(1);
    let threads = (cores / workers).max(1);

    Plan {
        workers,
        threads,
        width,
        height,
        supersample,
    }
}

fn default_tier(cores: u32, memory_limit: u64) -> (u32, u32, u32) {
    if cores <= 1 {
        (1024, 1024, 1)
    } else if cores <= 3 {
        (1024, 1024, 2)
    } else if memory_limit >= 1536 * MIB {
        (2048, 2048, 2)
    } else {
        (1024, 1024, 2)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Limits {
    memory_limit: u64,
    memory_available: u64,
    cpu_quota_milli: Option<u32>,
}

#[cfg(target_os = "linux")]
fn platform_limits() -> Limits {
    let memory_limit = read_first(
        &memory_paths("memory.max", "memory.limit_in_bytes"),
        parse_limit,
    );
    let memory_current = read_first(
        &memory_paths("memory.current", "memory.usage_in_bytes"),
        parse_number,
    );

    let (memory_limit, memory_available) = match memory_limit {
        Some(limit) => {
            let current = memory_current.unwrap_or(0);
            (limit, limit.saturating_sub(current))
        }
        None => match fs::read_to_string("/proc/meminfo") {
            Ok(text) => parse_meminfo(&text).unwrap_or((0, 0)),
            Err(_) => (0, 0),
        },
    };

    let cpu_quota_milli =
        read_first(&cpu_paths("cpu.max"), parse_cpu_max).or_else(|| read_v1_cpu_quota());

    Limits {
        memory_limit,
        memory_available,
        cpu_quota_milli,
    }
}

#[cfg(target_os = "linux")]
fn memory_paths(v2: &str, v1: &str) -> Vec<PathBuf> {
    let mut paths = vec![
        PathBuf::from("/sys/fs/cgroup").join(v2),
        PathBuf::from("/sys/fs/cgroup/memory").join(v1),
    ];
    for (controllers, path) in cgroup_entries() {
        if controllers.is_empty() {
            paths.push(PathBuf::from("/sys/fs/cgroup").join(&path).join(v2));
        } else if controllers.split(',').any(|item| item == "memory") {
            paths.push(PathBuf::from("/sys/fs/cgroup/memory").join(&path).join(v1));
        }
    }
    paths
}

#[cfg(target_os = "linux")]
fn cpu_paths(v2: &str) -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/sys/fs/cgroup").join(v2)];
    for (controllers, path) in cgroup_entries() {
        if controllers.is_empty() {
            paths.push(PathBuf::from("/sys/fs/cgroup").join(&path).join(v2));
        }
    }
    paths
}

/// Parse `/proc/self/cgroup` into `(controllers, path)` pairs.
#[cfg(target_os = "linux")]
fn cgroup_entries() -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string("/proc/self/cgroup") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, ':');
        let (Some(_id), Some(controllers), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        entries.push((controllers.to_string(), path.trim_matches('/').to_string()));
    }
    entries
}

#[cfg(target_os = "linux")]
fn read_v1_cpu_quota() -> Option<u32> {
    let quota = read_first(
        &[PathBuf::from("/sys/fs/cgroup/cpu/cpu.cfs_quota_us")],
        parse_number,
    )?;
    let period = read_first(
        &[PathBuf::from("/sys/fs/cgroup/cpu/cpu.cfs_period_us")],
        parse_number,
    )?;
    let period = u32::try_from(period).ok()?;
    let quota = i64::try_from(quota).ok()?;
    if quota <= 0 || period == 0 {
        return None;
    }
    Some((quota as u64 * 1000 / u64::from(period)) as u32)
}

#[cfg(target_os = "linux")]
fn read_first<T>(paths: &[PathBuf], parse: impl Fn(&str) -> Option<T>) -> Option<T> {
    paths
        .iter()
        .filter_map(|path| fs::read_to_string(path).ok())
        .find_map(|text| parse(&text))
}

/// `max` and the cgroup v1 "unlimited" sentinel both mean no limit.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_limit(text: &str) -> Option<u64> {
    let text = text.trim();
    if text == "max" {
        return None;
    }
    let value = text.parse::<u64>().ok()?;
    if value >= UNLIMITED_FLOOR {
        return None;
    }
    Some(value)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_number(text: &str) -> Option<u64> {
    text.trim().parse::<u64>().ok()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_cpu_max(text: &str) -> Option<u32> {
    let mut parts = text.split_whitespace();
    let quota = parts.next()?;
    let period = parts.next()?.parse::<u64>().ok()?;
    if quota == "max" || period == 0 {
        return None;
    }
    let quota = quota.parse::<u64>().ok()?;
    Some((quota * 1000 / period) as u32)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else { continue };
        let Some(value) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
            continue;
        };
        match key {
            "MemTotal:" => total = Some(value * 1024),
            "MemAvailable:" => available = Some(value * 1024),
            _ => {}
        }
    }
    Some((total?, available.unwrap_or(total?)))
}

#[cfg(windows)]
fn platform_limits() -> Limits {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }

    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };

    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    if !ok {
        return Limits::default();
    }

    Limits {
        memory_limit: status.total_phys,
        memory_available: status.avail_phys,
        cpu_quota_milli: None,
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
fn platform_limits() -> Limits {
    Limits::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unlimited_memory() {
        assert_eq!(parse_limit("max\n"), None);
        assert_eq!(parse_limit("9223372036854771712"), None);
        assert_eq!(parse_limit("1073741824"), Some(GIB));
    }

    #[test]
    fn parses_cpu_quota() {
        assert_eq!(parse_cpu_max("200000 100000"), Some(2000));
        assert_eq!(parse_cpu_max("150000 100000"), Some(1500));
        assert_eq!(parse_cpu_max("max 100000"), None);
        assert_eq!(parse_cpu_max("broken"), None);
    }

    #[test]
    fn parses_meminfo_in_bytes() {
        let text = "MemTotal:       2048000 kB\nMemAvailable:   1024000 kB\n";
        assert_eq!(parse_meminfo(text), Some((2048000 * 1024, 1024000 * 1024)));
    }

    #[test]
    fn plans_for_small_machines() {
        let small = plan(1, 2 * GIB);
        assert_eq!((small.workers, small.threads), (1, 1));
        assert_eq!((small.width, small.supersample), (1024, 1));

        let two = plan(2, 2 * GIB);
        assert_eq!((two.workers, two.threads), (1, 2));
        assert_eq!((two.width, two.supersample), (1024, 2));
    }

    #[test]
    fn plans_for_desktop_and_containers() {
        let desktop = plan(4, 8 * GIB);
        assert_eq!((desktop.workers, desktop.threads), (2, 2));
        assert_eq!((desktop.width, desktop.supersample), (2048, 2));

        let server = plan(12, 32 * GIB);
        assert_eq!((server.workers, server.threads), (4, 3));

        let container = plan(2, GIB);
        assert_eq!((container.workers, container.threads), (1, 2));
        assert_eq!((container.width, container.supersample), (1024, 2));
    }

    #[test]
    fn memory_ceiling_caps_workers() {
        // Eight cores would suggest four workers, but one gigabyte only fits two.
        let capped = plan(8, GIB);
        assert_eq!(capped.workers, 2);
        assert_eq!(capped.threads, 4);
    }

    #[test]
    fn unknown_memory_falls_back_to_cpu() {
        let unknown = plan(12, 0);
        assert_eq!(unknown.workers, 4);
        assert_eq!(unknown.threads, 3);
    }
}
