use super::geometry::bounds;
use super::project::Triangle;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::PathBuf;

/// Triangles retained in memory before translucent spill starts.
pub(super) const DEFAULT_TRANSLUCENT_BUDGET: usize = 262_144;

/// Tiles per bucket edge. A bucket is read and rendered as one bounded batch.
const BUCKET_TILES: u32 = 4;
const WRITE_BUFFER: usize = 64 * 1024;
const READ_BUFFER: usize = 64 * 1024;

pub(super) struct TranslucentStore {
    budget: usize,
    tiles_x: u32,
    tiles_y: u32,
    tile_size: u32,
    state: State,
}

enum State {
    Memory(Vec<Triangle>),
    Spilled(SpillStore),
}

impl TranslucentStore {
    pub(super) fn new(budget: usize, tiles_x: u32, tiles_y: u32, tile_size: u32) -> Self {
        Self {
            budget,
            tiles_x,
            tiles_y,
            tile_size,
            state: State::Memory(Vec::new()),
        }
    }

    pub(super) fn push_many(&mut self, triangles: Vec<Triangle>) -> io::Result<()> {
        #[cfg(target_arch = "wasm32")]
        if let State::Memory(memory) = &self.state
            && memory.len() + triangles.len() > self.budget
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "translucent spill requires a filesystem; lower lod or resolution",
            ));
        }
        match &mut self.state {
            State::Memory(memory) if memory.len() + triangles.len() <= self.budget => {
                memory.extend(triangles);
                Ok(())
            }
            State::Memory(_) => {
                let mut spill = SpillStore::new(self.tiles_x, self.tiles_y, self.tile_size)?;
                let State::Memory(mut memory) =
                    std::mem::replace(&mut self.state, State::Memory(Vec::new()))
                else {
                    unreachable!("state is memory");
                };
                for triangle in memory.drain(..) {
                    spill.push(triangle)?;
                }
                for triangle in triangles {
                    spill.push(triangle)?;
                }
                self.state = State::Spilled(spill);
                Ok(())
            }
            State::Spilled(spill) => {
                for triangle in triangles {
                    spill.push(triangle)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn finish(self) -> io::Result<TranslucentParts> {
        match self.state {
            State::Memory(triangles) => Ok(TranslucentParts::Memory(triangles)),
            State::Spilled(spill) => spill.finish().map(TranslucentParts::Spilled),
        }
    }
}

pub(super) enum TranslucentParts {
    Memory(Vec<Triangle>),
    Spilled(SpillReader),
}

struct SpillStore {
    dir: tempfile::TempDir,
    writers: Vec<BufWriter<File>>,
    tiles_x: u32,
    tiles_y: u32,
    tile_size: f64,
    bucket_cols: u32,
}

impl SpillStore {
    fn new(tiles_x: u32, tiles_y: u32, tile_size: u32) -> io::Result<Self> {
        if tiles_x == 0 || tiles_y == 0 || tile_size == 0 {
            return Err(invalid("translucent spill has no tiles"));
        }
        let dir = tempfile::TempDir::new()?;
        let bucket_cols = tiles_x.div_ceil(BUCKET_TILES);
        let bucket_rows = tiles_y.div_ceil(BUCKET_TILES);
        let mut writers = Vec::with_capacity((bucket_cols * bucket_rows) as usize);
        for index in 0..bucket_cols * bucket_rows {
            let path = dir.path().join(format!("bucket-{index}.bin"));
            let file = File::create(path)?;
            writers.push(BufWriter::with_capacity(WRITE_BUFFER, file));
        }

        Ok(Self {
            dir,
            writers,
            tiles_x,
            tiles_y,
            tile_size: f64::from(tile_size),
            bucket_cols,
        })
    }

    fn push(&mut self, triangle: Triangle) -> io::Result<()> {
        let points = triangle.points_f64();
        let bbox = bounds(&points);
        let max_tile_x = f64::from(self.tiles_x - 1);
        let max_tile_y = f64::from(self.tiles_y - 1);
        let min_x = (bbox[0] / self.tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let max_x = (bbox[2] / self.tile_size).floor().clamp(0.0, max_tile_x) as u32;
        let min_y = (bbox[1] / self.tile_size).floor().clamp(0.0, max_tile_y) as u32;
        let max_y = (bbox[3] / self.tile_size).floor().clamp(0.0, max_tile_y) as u32;

        for tile_y in min_y..=max_y {
            for tile_x in min_x..=max_x {
                let tile_index = tile_y * self.tiles_x + tile_x;
                let bucket = (tile_y / BUCKET_TILES) * self.bucket_cols + tile_x / BUCKET_TILES;
                let record = triangle.encode_record(tile_index);
                self.writers[bucket as usize].write_all(&record)?;
            }
        }
        Ok(())
    }

    fn finish(self) -> io::Result<SpillReader> {
        let Self {
            dir,
            mut writers,
            tiles_x,
            tiles_y,
            tile_size: _,
            bucket_cols,
        } = self;
        for writer in &mut writers {
            writer.flush()?;
        }
        drop(writers);

        let bucket_rows = tiles_y.div_ceil(BUCKET_TILES);
        Ok(SpillReader {
            _dir: dir,
            tiles_x,
            tiles_y,
            bucket_count: bucket_cols * bucket_rows,
            next_bucket: 0,
        })
    }
}

pub(super) struct SpillReader {
    _dir: tempfile::TempDir,
    tiles_x: u32,
    tiles_y: u32,
    bucket_count: u32,
    next_bucket: u32,
}

impl SpillReader {
    /// Read the next bucket, or `None` after the last one.
    pub(super) fn read_next(&mut self) -> io::Result<Option<BTreeMap<u32, Vec<Triangle>>>> {
        if self.next_bucket >= self.bucket_count {
            return Ok(None);
        }
        let bucket = self.next_bucket;
        self.next_bucket += 1;
        let path: PathBuf = self._dir.path().join(format!("bucket-{bucket}.bin"));
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        let record_size = Triangle::RECORD_BYTES as u64;
        if length % record_size != 0 {
            return Err(invalid("translucent spill has a truncated record"));
        }

        let mut reader = BufReader::with_capacity(READ_BUFFER, file);
        let mut records = BTreeMap::new();
        let mut buffer = [0u8; Triangle::RECORD_BYTES];
        for _ in 0..length / record_size {
            reader.read_exact(&mut buffer)?;
            let (tile_index, triangle) = Triangle::decode_record(&buffer);
            if tile_index >= self.tiles_x * self.tiles_y {
                return Err(invalid("translucent spill has an invalid tile"));
            }
            records
                .entry(tile_index)
                .or_insert_with(Vec::new)
                .push(triangle);
        }
        Ok(Some(records))
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
