//! Standalone prototype for a streaming litematic `BlockStates` scan.
//!
//! This is a measurement tool, not a production parser. It scans the gzip
//! stream twice: the first pass records each region's header and the byte
//! offset of its `BlockStates` array, and the second pass decodes those words
//! without retaining the decompressed document or the long array.
//!
//! Run with:
//! `cargo run --release -p litematic-core --example sparse_probe -- <file> [--dense]`

use flate2::read::GzDecoder;
use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use std::alloc::{GlobalAlloc, Layout, System};
use std::env;
use std::fs;
use std::io::{self, BufReader, Cursor, Read};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const MAX_DEPTH: usize = 64;
const BUFFER_SIZE: usize = 64 * 1024;

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
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        record_dealloc(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
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

struct Reader<R> {
    inner: R,
    offset: u64,
}

impl<R: Read> Reader<R> {
    fn new(inner: R) -> Self {
        Self { inner, offset: 0 }
    }

    fn read_u8(&mut self) -> io::Result<u8> {
        let mut buffer = [0u8; 1];
        self.inner.read_exact(&mut buffer)?;
        self.offset += 1;
        Ok(buffer[0])
    }

    fn read_u16(&mut self) -> io::Result<u16> {
        let mut buffer = [0u8; 2];
        self.inner.read_exact(&mut buffer)?;
        self.offset += 2;
        Ok(u16::from_be_bytes(buffer))
    }

    fn read_i32(&mut self) -> io::Result<i32> {
        let mut buffer = [0u8; 4];
        self.inner.read_exact(&mut buffer)?;
        self.offset += 4;
        Ok(i32::from_be_bytes(buffer))
    }

    fn read_u64(&mut self) -> io::Result<u64> {
        let mut buffer = [0u8; 8];
        self.inner.read_exact(&mut buffer)?;
        self.offset += 8;
        Ok(u64::from_be_bytes(buffer))
    }

    fn read_name(&mut self) -> io::Result<String> {
        let length = usize::from(self.read_u16()?);
        if length > 1024 * 1024 {
            return Err(invalid("NBT name is too long"));
        }
        let mut buffer = vec![0u8; length];
        self.inner.read_exact(&mut buffer)?;
        self.offset += length as u64;
        String::from_utf8(buffer).map_err(|_| invalid("NBT name is not UTF-8"))
    }

    fn skip(&mut self, length: u64) -> io::Result<()> {
        let mut remaining = length;
        let mut buffer = [0u8; 8192];
        while remaining > 0 {
            let take = usize::try_from(remaining.min(buffer.len() as u64))
                .map_err(|_| invalid("skip length overflow"))?;
            self.inner.read_exact(&mut buffer[..take])?;
            self.offset += take as u64;
            remaining -= take as u64;
        }
        Ok(())
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[derive(Debug)]
struct RegionScan {
    volume: u64,
    bits: u8,
    word_offset: u64,
}

fn open_stream(bytes: &[u8]) -> Reader<BufReader<GzDecoder<Cursor<&[u8]>>>> {
    Reader::new(BufReader::with_capacity(
        BUFFER_SIZE,
        GzDecoder::new(Cursor::new(bytes)),
    ))
}

fn scan_litematic(bytes: &[u8]) -> io::Result<Vec<RegionScan>> {
    let mut reader = open_stream(bytes);
    if reader.read_u8()? != 10 {
        return Err(invalid("root is not a compound"));
    }
    let _ = reader.read_name()?;

    let mut regions = Vec::new();
    loop {
        let tag = reader.read_u8()?;
        if tag == 0 {
            break;
        }
        let name = reader.read_name()?;
        if tag == 10 && name == "Regions" {
            scan_regions(&mut reader, &mut regions)?;
        } else {
            skip_payload(&mut reader, tag, 1)?;
        }
    }
    Ok(regions)
}

fn scan_regions<R: Read>(reader: &mut Reader<R>, regions: &mut Vec<RegionScan>) -> io::Result<()> {
    loop {
        let tag = reader.read_u8()?;
        if tag == 0 {
            break;
        }
        let _name = reader.read_name()?;
        if tag == 10 {
            regions.push(scan_region(reader)?);
        } else {
            skip_payload(reader, tag, 2)?;
        }
    }
    Ok(())
}

fn scan_region<R: Read>(reader: &mut Reader<R>) -> io::Result<RegionScan> {
    let mut size = [0i32; 3];
    let mut palette_len = 0usize;
    let mut word_offset = 0u64;
    let mut word_count = 0usize;

    loop {
        let tag = reader.read_u8()?;
        if tag == 0 {
            break;
        }
        let name = reader.read_name()?;
        match (name.as_str(), tag) {
            ("Size", 10) => size = scan_size(reader)?,
            ("BlockStatePalette", 9) => palette_len = scan_palette(reader)?,
            ("BlockStates", 12) => {
                let length = reader.read_i32()?;
                if length < 0 {
                    return Err(invalid("negative BlockStates length"));
                }
                word_count = usize::try_from(length).map_err(|_| invalid("word count"))?;
                word_offset = reader.offset;
                reader.skip((word_count as u64) * 8)?;
            }
            _ => skip_payload(reader, tag, 2)?,
        }
    }

    let volume = size.iter().try_fold(1u64, |total, value| {
        let value = u64::from(value.unsigned_abs());
        total.checked_mul(value)
    });
    let volume = volume.ok_or_else(|| invalid("region volume overflow"))?;
    if volume == 0 || palette_len == 0 || word_count == 0 {
        return Err(invalid("region is missing Size, palette, or BlockStates"));
    }

    let bits = (usize::BITS - (palette_len - 1).leading_zeros()).max(2) as u8;
    let expected = (volume * u64::from(bits)).div_ceil(64);
    if expected != word_count as u64 {
        return Err(invalid("BlockStates length does not match the region"));
    }

    Ok(RegionScan {
        volume,
        bits,
        word_offset,
    })
}

fn scan_size<R: Read>(reader: &mut Reader<R>) -> io::Result<[i32; 3]> {
    let mut size = [0i32; 3];
    loop {
        let tag = reader.read_u8()?;
        if tag == 0 {
            break;
        }
        let name = reader.read_name()?;
        if tag == 3 {
            let value = reader.read_i32()?;
            match name.as_str() {
                "x" => size[0] = value,
                "y" => size[1] = value,
                "z" => size[2] = value,
                _ => {}
            }
        } else {
            skip_payload(reader, tag, 3)?;
        }
    }
    Ok(size)
}

fn scan_palette<R: Read>(reader: &mut Reader<R>) -> io::Result<usize> {
    let element = reader.read_u8()?;
    let length = reader.read_i32()?;
    if element != 10 || length < 0 {
        return Err(invalid("BlockStatePalette is not a compound list"));
    }
    for _ in 0..length {
        skip_payload(reader, 10, 3)?;
    }
    usize::try_from(length).map_err(|_| invalid("palette count"))
}

fn skip_payload<R: Read>(reader: &mut Reader<R>, tag: u8, depth: usize) -> io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(invalid("NBT depth limit exceeded"));
    }

    match tag {
        1 => reader.skip(1),
        2 => reader.skip(2),
        3 => reader.skip(4),
        4 => reader.skip(8),
        5 => reader.skip(4),
        6 => reader.skip(8),
        7 => {
            let length = reader.read_i32()?;
            if length < 0 {
                return Err(invalid("negative byte array length"));
            }
            reader.skip(length as u64)
        }
        8 => {
            let length = reader.read_u16()?;
            reader.skip(u64::from(length))
        }
        9 => {
            let element = reader.read_u8()?;
            let length = reader.read_i32()?;
            if length < 0 || (element == 0 && length > 0) {
                return Err(invalid("invalid list length"));
            }
            for _ in 0..length {
                skip_payload(reader, element, depth + 1)?;
            }
            Ok(())
        }
        10 => {
            loop {
                let child = reader.read_u8()?;
                if child == 0 {
                    break;
                }
                let _name = reader.read_name()?;
                skip_payload(reader, child, depth + 1)?;
            }
            Ok(())
        }
        11 => {
            let length = reader.read_i32()?;
            if length < 0 {
                return Err(invalid("negative int array length"));
            }
            reader.skip((length as u64) * 4)
        }
        12 => {
            let length = reader.read_i32()?;
            if length < 0 {
                return Err(invalid("negative long array length"));
            }
            reader.skip((length as u64) * 8)
        }
        _ => Err(invalid("unknown NBT tag")),
    }
}

fn count_nonzero(bytes: &[u8], region: &RegionScan) -> io::Result<u64> {
    let mut reader = open_stream(bytes);
    reader.skip(region.word_offset)?;

    let bits = u32::from(region.bits);
    let mask = (1u64 << bits) - 1;
    let mut buffer = 0u64;
    let mut buffered = 0u32;
    let mut remaining = region.volume;
    let mut occupied = 0u64;

    while remaining > 0 {
        let value = if buffered >= bits {
            let value = buffer & mask;
            buffer >>= bits;
            buffered -= bits;
            value
        } else {
            let need = bits - buffered;
            let word = reader.read_u64()?;
            let low = if buffered == 0 {
                0
            } else {
                buffer & ((1u64 << buffered) - 1)
            };
            let value = low | ((word & ((1u64 << need) - 1)) << buffered);
            buffer = word >> need;
            buffered = 64 - need;
            value
        };

        if value != 0 {
            occupied += 1;
        }
        remaining -= 1;
    }

    Ok(occupied)
}

fn dense_limits() -> DecodeLimits {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let path = args.next().ok_or("usage: sparse_probe <file> [--dense]")?;
    let dense = args.any(|arg| arg == "--dense");

    let bytes = fs::read(&path)?;
    println!("file: {path} ({} bytes compressed)", bytes.len());

    let (regions, stream_peak) = measure(|| scan_litematic(&bytes));
    let regions = regions?;
    let stream_started = Instant::now();
    let (occupied, word_peak) = measure(|| {
        regions
            .iter()
            .map(|region| count_nonzero(&bytes, region))
            .collect::<io::Result<Vec<_>>>()
    });
    let occupied: u64 = occupied?.iter().sum();

    println!(
        "stream: regions={} occupied_nonzero={} scan_peak={} B decode_peak={} B decode_ms={:.1}",
        regions.len(),
        occupied,
        stream_peak,
        word_peak,
        stream_started.elapsed().as_secs_f64() * 1000.0
    );

    if dense {
        let started = Instant::now();
        let (file, dense_peak) =
            measure(|| LitematicFile::parse_bytes(&bytes, &dense_limits(), &litematic_limits()));
        let file = file?;
        let blocks = file.regions.iter().map(|region| region.volume).sum::<u64>();
        drop(file);
        println!(
            "dense: regions={} volume={} parse_peak={} B parse_ms={:.1}",
            regions.len(),
            blocks,
            dense_peak,
            started.elapsed().as_secs_f64() * 1000.0
        );
    }

    Ok(())
}
