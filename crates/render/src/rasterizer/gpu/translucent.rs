use super::super::project::Triangle;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};

const DEFAULT_BUCKETS: usize = 128;
const DEFAULT_IN_MEMORY: usize = 262_144;
const BUCKET_READ_LIMIT: usize = 262_144;
const WRITE_BUFFER: usize = 64 * 1024;
const READ_BUFFER: usize = 64 * 1024;

/// Bounded translucent storage. Small scenes stay in memory; larger ones are
/// spilled into coarse camera-depth buckets and read far-to-near.
pub(super) struct TranslucentStore {
    depth_min: f64,
    depth_span: f64,
    bucket_count: usize,
    in_memory_limit: usize,
    state: State,
}

enum State {
    Memory(Vec<Triangle>),
    Spilled(SpillStore),
}

impl TranslucentStore {
    pub(super) fn new(depth_min: f64, depth_max: f64) -> Self {
        Self::with_options(depth_min, depth_max, DEFAULT_BUCKETS, DEFAULT_IN_MEMORY)
    }

    fn with_options(
        depth_min: f64,
        depth_max: f64,
        bucket_count: usize,
        in_memory_limit: usize,
    ) -> Self {
        Self {
            depth_min,
            depth_span: (depth_max - depth_min).max(1e-6),
            bucket_count: bucket_count.max(1),
            in_memory_limit,
            state: State::Memory(Vec::new()),
        }
    }

    pub(super) fn push_many(&mut self, mut triangles: Vec<Triangle>) -> io::Result<()> {
        if triangles.is_empty() {
            return Ok(());
        }
        let depth_min = self.depth_min;
        let depth_span = self.depth_span;
        let bucket_count = self.bucket_count;
        let bucket = |depth| bucket_of(depth, depth_min, depth_span, bucket_count);
        match &mut self.state {
            State::Memory(memory)
                if memory.len().saturating_add(triangles.len()) <= self.in_memory_limit =>
            {
                memory.append(&mut triangles);
                Ok(())
            }
            State::Memory(_) => {
                let mut spill = SpillStore::new(self.bucket_count)?;
                let State::Memory(mut memory) =
                    std::mem::replace(&mut self.state, State::Memory(Vec::new()))
                else {
                    unreachable!("state is memory");
                };
                for triangle in memory.drain(..) {
                    let target = bucket(triangle.depth_key);
                    spill.push(triangle, target)?;
                }
                for triangle in triangles {
                    let target = bucket(triangle.depth_key);
                    spill.push(triangle, target)?;
                }
                self.state = State::Spilled(spill);
                Ok(())
            }
            State::Spilled(spill) => {
                for triangle in triangles {
                    let target = bucket(triangle.depth_key);
                    spill.push(triangle, target)?;
                }
                Ok(())
            }
        }
    }

    pub(super) fn finish(&mut self) -> io::Result<TranslucentParts> {
        let state = std::mem::replace(&mut self.state, State::Memory(Vec::new()));
        match state {
            State::Memory(mut triangles) => {
                triangles.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));
                Ok(TranslucentParts::Memory(triangles))
            }
            State::Spilled(spill) => spill.finish().map(TranslucentParts::Spilled),
        }
    }
}

fn bucket_of(depth_key: f64, depth_min: f64, depth_span: f64, bucket_count: usize) -> usize {
    let t = ((depth_key - depth_min) / depth_span).clamp(0.0, 1.0);
    ((t * (bucket_count - 1) as f64).round() as usize).min(bucket_count - 1)
}

pub(super) enum TranslucentParts {
    /// Already sorted far-to-near.
    Memory(Vec<Triangle>),
    Spilled(SpillReader),
}

struct SpillStore {
    dir: tempfile::TempDir,
    writers: Vec<BufWriter<File>>,
}

impl SpillStore {
    fn new(bucket_count: usize) -> io::Result<Self> {
        let dir = tempfile::TempDir::new()?;
        let mut writers = Vec::with_capacity(bucket_count);
        for index in 0..bucket_count {
            let path = dir.path().join(format!("bucket-{index}.bin"));
            let file = File::create(path)?;
            writers.push(BufWriter::with_capacity(WRITE_BUFFER, file));
        }
        Ok(Self { dir, writers })
    }

    fn push(&mut self, triangle: Triangle, bucket: usize) -> io::Result<()> {
        self.writers[bucket].write_all(&triangle.encode_depth_record())
    }

    fn finish(self) -> io::Result<SpillReader> {
        let bucket_count = self.writers.len();
        let Self { dir, mut writers } = self;
        for writer in &mut writers {
            writer.flush()?;
        }
        drop(writers);
        Ok(SpillReader {
            _dir: dir,
            bucket_count,
            next_bucket: 0,
            current: None,
        })
    }
}

struct CurrentBucket {
    reader: BufReader<File>,
    remaining: u64,
}

pub(super) struct SpillReader {
    _dir: tempfile::TempDir,
    bucket_count: usize,
    next_bucket: usize,
    current: Option<CurrentBucket>,
}

impl SpillReader {
    /// Read the next bounded sub-batch, walking buckets far-to-near.
    pub(super) fn read_next(&mut self) -> io::Result<Option<Vec<Triangle>>> {
        loop {
            if let Some(current) = &mut self.current {
                if current.remaining == 0 {
                    self.current = None;
                    continue;
                }
                let take = current.remaining.min(BUCKET_READ_LIMIT as u64) as usize;
                let mut out = Vec::with_capacity(take);
                let mut buffer = [0u8; Triangle::RECORD_BYTES];
                for _ in 0..take {
                    current.reader.read_exact(&mut buffer)?;
                    out.push(Triangle::decode_depth_record(&buffer));
                }
                current.remaining -= take as u64;
                return Ok(Some(out));
            }

            if self.next_bucket >= self.bucket_count {
                return Ok(None);
            }
            let bucket = self.bucket_count - 1 - self.next_bucket;
            self.next_bucket += 1;
            let path = self._dir.path().join(format!("bucket-{bucket}.bin"));
            let file = File::open(path)?;
            let length = file.metadata()?.len();
            let record = Triangle::RECORD_BYTES as u64;
            if length % record != 0 {
                return Err(invalid("translucent bucket has a truncated record"));
            }
            if length == 0 {
                continue;
            }
            self.current = Some(CurrentBucket {
                reader: BufReader::with_capacity(READ_BUFFER, file),
                remaining: length / record,
            });
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::TranslucentStore;
    use crate::rasterizer::project::Triangle;

    fn store(limit: usize) -> TranslucentStore {
        TranslucentStore::with_options(0.0, 10.0, 4, limit)
    }

    #[test]
    fn memory_path_sorts_far_to_near() {
        let mut store = store(16);
        store
            .push_many(vec![
                Triangle::for_test(1.0),
                Triangle::for_test(9.0),
                Triangle::for_test(5.0),
            ])
            .unwrap();
        let super::TranslucentParts::Memory(triangles) = store.finish().unwrap() else {
            panic!("small store should stay in memory");
        };
        let depths: Vec<f64> = triangles
            .iter()
            .map(|triangle| triangle.depth_key)
            .collect();
        assert_eq!(depths, vec![9.0, 5.0, 1.0]);
    }

    #[test]
    fn spilled_files_are_removed_with_the_reader() {
        let mut store = store(1);
        store
            .push_many(vec![Triangle::for_test(1.0), Triangle::for_test(9.0)])
            .unwrap();
        let super::TranslucentParts::Spilled(reader) = store.finish().unwrap() else {
            panic!("store should spill");
        };
        let path = reader._dir.path().to_path_buf();
        assert!(path.exists());
        drop(reader);
        assert!(!path.exists(), "temporary bucket directory must be removed");
    }

    #[test]
    fn spill_path_reads_buckets_far_to_near() {
        let mut store = store(2);
        store
            .push_many(
                (0..12)
                    .map(|index| Triangle::for_test(f64::from(index) * 0.8))
                    .collect(),
            )
            .unwrap();
        let super::TranslucentParts::Spilled(mut reader) = store.finish().unwrap() else {
            panic!("large store should spill");
        };
        let mut depths = Vec::new();
        while let Some(mut batch) = reader.read_next().unwrap() {
            batch.sort_by(|left, right| right.depth_key.total_cmp(&left.depth_key));
            depths.extend(batch.iter().map(|triangle| triangle.depth_key));
        }
        assert_eq!(depths.len(), 12);
        assert!(
            depths.windows(2).all(|pair| pair[0] >= pair[1]),
            "bucketed output must be far-to-near: {depths:?}"
        );
    }
}
