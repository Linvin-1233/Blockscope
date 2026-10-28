use std::process::Child;

/// Sample the worker's resident set size without sending it a signal.
///
/// Linux exposes the high-water mark through `/proc`; Windows reports the
/// current working set through `K32GetProcessMemoryInfo`; macOS uses
/// `proc_pidinfo`. Other platforms return `None`, which disables the memory
/// ceiling rather than guessing.
#[cfg(target_os = "linux")]
pub(super) fn process_rss_bytes(child: &Child) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmRSS:") {
            let kb = value.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(windows)]
pub(super) fn process_rss_bytes(child: &Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    let handle = child.as_raw_handle();
    // SAFETY: `handle` is a live process handle owned by `child`, and the
    // counters struct is `#[repr(C)]` with `cb` initialized to its size.
    let ok = unsafe { K32GetProcessMemoryInfo(handle, &mut counters, counters.cb) };
    if ok == 0 {
        None
    } else {
        Some(counters.working_set_size as u64)
    }
}

#[cfg(target_os = "macos")]
pub(super) fn process_rss_bytes(child: &Child) -> Option<u64> {
    #[repr(C)]
    struct ProcTaskInfo {
        pti_virtual_size: u64,
        pti_resident_size: u64,
        pti_total_user: u64,
        pti_total_system: u64,
        pti_threads_user: u64,
        pti_threads_system: u64,
        pti_policy: i32,
        pti_faults: i32,
        pti_pageins: i32,
        pti_cow_faults: i32,
        pti_messages_sent: i32,
        pti_messages_received: i32,
        pti_syscalls_mach: i32,
        pti_syscalls_unix: i32,
        pti_csw: i32,
        pti_threadnum: i32,
        pti_numrunning: i32,
        pti_priority: i32,
    }

    const PROC_PIDTASKINFO: i32 = 4;

    unsafe extern "C" {
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            buffersize: i32,
        ) -> i32;
    }

    // SAFETY: `ProcTaskInfo` is a plain C-layout struct, so an all-zero value
    // is a valid initialization before `proc_pidinfo` fills it.
    let mut info: ProcTaskInfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<ProcTaskInfo>() as i32;
    // SAFETY: the process id comes from `child`, and `info` is a live
    // `#[repr(C)]` buffer whose size is passed alongside it.
    let ret = unsafe {
        proc_pidinfo(
            child.id() as i32,
            PROC_PIDTASKINFO,
            0,
            &mut info as *mut _ as *mut std::ffi::c_void,
            size,
        )
    };
    if ret == size {
        Some(info.pti_resident_size)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
pub(super) fn process_rss_bytes(_child: &Child) -> Option<u64> {
    None
}
