//! Allocation-peak regression tests for the NBT decoder's pre-allocation guards.
//!
//! The meter is process-global, so the measured scenarios share this one test
//! binary and run in a single test. Fixtures are built before the measured
//! window; the assertions cover allocations made by `decode` itself.

use litematic_nbt::{DecodeLimits, NbtErrorCode, decode};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

const MB: usize = 1024 * 1024;

struct AllocationMeter;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn record_alloc(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

fn record_dealloc(size: usize) {
    LIVE.fetch_sub(size, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for AllocationMeter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: delegating to the system allocator with the same layout.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: delegating to the system allocator with the same layout.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer came from this allocator and is still live.
        unsafe { System.dealloc(pointer, layout) };
        record_dealloc(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: the pointer and layout describe a live allocation from this
        // allocator, and `size` is the requested replacement size.
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() {
            if size >= layout.size() {
                record_alloc(size - layout.size());
            } else {
                record_dealloc(layout.size() - size);
            }
        }
        next
    }
}

#[global_allocator]
static ALLOCATOR: AllocationMeter = AllocationMeter;

fn measure<T>(scenario: impl FnOnce() -> T) -> (T, usize) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let value = scenario();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    (value, peak)
}

fn named(tag: u8, name: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(payload);
    out
}

fn root(children: &[u8]) -> Vec<u8> {
    let mut out = vec![10, 0, 0];
    out.extend_from_slice(children);
    out.push(0);
    out
}

#[test]
fn impossible_declared_lengths_allocate_far_less_than_the_claim() {
    let mut int_array = Vec::new();
    int_array.extend_from_slice(&64_000_000i32.to_be_bytes());
    let array_input = root(&named(11, "ints", &int_array));

    let (result, peak) = measure(|| decode(&array_input, &DecodeLimits::default()));
    assert_eq!(result.unwrap_err().code, NbtErrorCode::TruncatedInput);
    assert!(
        peak < MB,
        "int array guard allocated {peak} bytes for a truncated input"
    );

    let mut list = vec![3u8];
    list.extend_from_slice(&1_000_000i32.to_be_bytes());
    let list_input = root(&named(9, "list", &list));

    let (result, peak) = measure(|| decode(&list_input, &DecodeLimits::default()));
    assert_eq!(result.unwrap_err().code, NbtErrorCode::TruncatedInput);
    assert!(
        peak < MB,
        "list guard allocated {peak} bytes for a truncated input"
    );
}
