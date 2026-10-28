//! Allocation-peak regression tests for litematic parsing and scene building.
//!
//! The meter is process-global, so every measured scenario lives in this one
//! test binary and runs sequentially. Fixtures are compressed before the
//! measured window; the numbers below are peaks above the live baseline.

use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

const KB: usize = 1024;
const MB: usize = 1024 * KB;

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

fn nbt_limits() -> DecodeLimits {
    DecodeLimits {
        max_input_bytes: 1 << 30,
        max_decompressed_bytes: 1 << 30,
        max_total_bytes: 1 << 30,
        ..DecodeLimits::default()
    }
}

fn litematic_limits() -> LitematicLimits {
    LitematicLimits {
        max_volume_per_region: 1 << 40,
        max_total_volume: 1 << 40,
        ..LitematicLimits::default()
    }
}

fn named(tag: u8, name: &str) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(name.len() as u16).to_be_bytes());
    out.extend_from_slice(name.as_bytes());
    out
}

fn int_tag(name: &str, value: i32) -> Vec<u8> {
    let mut out = named(3, name);
    out.extend_from_slice(&value.to_be_bytes());
    out
}

fn string_tag(name: &str, value: &str) -> Vec<u8> {
    let mut out = named(8, name);
    out.extend_from_slice(&(value.len() as u16).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
    out
}

fn compound_tag(name: &str, children: &[u8]) -> Vec<u8> {
    let mut out = named(10, name);
    out.extend_from_slice(children);
    out.push(0);
    out
}

fn empty_compound_list(name: &str) -> Vec<u8> {
    let mut out = named(9, name);
    out.extend_from_slice(&[10, 0, 0, 0, 0]);
    out
}

fn palette_entry(name: &str) -> Vec<u8> {
    let mut out = string_tag("Name", name);
    out.push(0);
    out
}

fn palette_list(entries: &[&str]) -> Vec<u8> {
    let mut payload = vec![10];
    payload.extend_from_slice(&(entries.len() as i32).to_be_bytes());
    for entry in entries {
        payload.extend_from_slice(&palette_entry(entry));
    }
    payload
}

fn vec3_compound(name: &str, values: [i32; 3]) -> Vec<u8> {
    let mut children = Vec::new();
    children.extend(int_tag("x", values[0]));
    children.extend(int_tag("y", values[1]));
    children.extend(int_tag("z", values[2]));
    compound_tag(name, &children)
}

fn long_array(name: &str, words: &[i64]) -> Vec<u8> {
    let mut out = named(12, name);
    out.extend_from_slice(&(words.len() as i32).to_be_bytes());
    for word in words {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}

fn single_region_litematic(name: &str, size: u32, palette: &[&str], words: &[i64]) -> Vec<u8> {
    let mut region = Vec::new();
    region.extend(vec3_compound("Position", [0, 0, 0]));
    region.extend(vec3_compound(
        "Size",
        [size as i32, size as i32, size as i32],
    ));
    region.extend(named(9, "BlockStatePalette"));
    region.extend(palette_list(palette));
    region.extend(empty_compound_list("TileEntities"));
    region.extend(empty_compound_list("Entities"));
    region.extend(empty_compound_list("PendingBlockTicks"));
    region.extend(empty_compound_list("PendingFluidTicks"));
    region.extend(long_array("BlockStates", words));

    let mut metadata = Vec::new();
    metadata.extend(string_tag("Name", name));
    let metadata = compound_tag("Metadata", &metadata);

    let mut regions = Vec::new();
    regions.extend(compound_tag("Main", &region));
    let regions = compound_tag("Regions", &regions);

    let mut root = vec![10, 0, 0];
    root.extend(int_tag("Version", 6));
    root.extend(int_tag("MinecraftDataVersion", 3465));
    root.extend(metadata);
    root.extend(regions);
    root.push(0);
    root
}

fn compress(bytes: &[u8]) -> Vec<u8> {
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Write;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

fn sparse_words(size: u32, blocks: &[(u32, u32, u32)]) -> Vec<i64> {
    let volume = u64::from(size) * u64::from(size) * u64::from(size);
    let long_count = (volume * 2).div_ceil(64);
    let mut words = vec![0i64; usize::try_from(long_count).unwrap()];

    for &(x, y, z) in blocks {
        let index = u64::from(x)
            + u64::from(z) * u64::from(size)
            + u64::from(y) * u64::from(size) * u64::from(size);
        let offset = index * 2;
        let word = usize::try_from(offset / 64).unwrap();
        let bit = (offset % 64) as u32;
        assert!(bit < 63, "fixture value must not cross a word");
        words[word] |= 1i64 << bit;
    }

    words
}

fn dense_words(size: u32) -> Vec<i64> {
    let volume = u64::from(size) * u64::from(size) * u64::from(size);
    let long_count = (volume * 2).div_ceil(64);
    // Every 2-bit value is 1: a region full of stone.
    vec![0x5555_5555_5555_5555u64 as i64; usize::try_from(long_count).unwrap()]
}

fn parse(bytes: &[u8]) -> LitematicFile {
    LitematicFile::parse_bytes(bytes, &nbt_limits(), &litematic_limits()).unwrap()
}

#[test]
fn parse_and_scene_peaks_are_bounded() {
    let tiny = compress(&single_region_litematic(
        "tiny",
        2,
        &["minecraft:air", "minecraft:stone"],
        &dense_words(2),
    ));
    let sparse256 = compress(&single_region_litematic(
        "sparse256",
        256,
        &["minecraft:air", "minecraft:stone"],
        &sparse_words(256, &[(1, 2, 3)]),
    ));
    let sparse512 = compress(&single_region_litematic(
        "sparse512",
        512,
        &["minecraft:air", "minecraft:stone"],
        &sparse_words(512, &[(1, 2, 3)]),
    ));
    let dense256 = compress(&single_region_litematic(
        "dense256",
        256,
        &["minecraft:air", "minecraft:stone"],
        &dense_words(256),
    ));

    let cases = [
        ("tiny", &tiny, 8 * MB),
        ("sparse256", &sparse256, 64 * MB),
        ("sparse512", &sparse512, 128 * MB),
        ("dense256", &dense256, 32 * MB),
    ];

    let mut sparse_peaks = Vec::new();
    for (name, bytes, budget) in cases {
        let (file, parse_peak) = measure(|| parse(bytes));
        drop(file);

        let (scene, scene_peak) = measure(|| parse(bytes).into_scene().unwrap());
        drop(scene);

        println!("{name}: parse={parse_peak} B, parse+scene={scene_peak} B");
        assert!(
            parse_peak <= budget,
            "{name} parse peak {parse_peak} exceeds budget {budget}"
        );
        assert!(
            scene_peak <= budget,
            "{name} parse+scene peak {scene_peak} exceeds budget {budget}"
        );

        if name.starts_with("sparse") {
            sparse_peaks.push((name, scene_peak));
        }
    }

    if let [(_, small), (_, large)] = sparse_peaks.as_slice() {
        println!(
            "sparse scale 512/256 = {:.2}x",
            *large as f64 / *small as f64
        );
    }
}
