use crate::{MaterialError, MaterialErrorCode};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_ASSET_BYTES: usize = 8 * 1024 * 1024;

/// Ordered asset lookup: overlay layers first, then the base source.
pub struct AssetSource {
    layers: Vec<Layer>,
}

enum Layer {
    Dir(PathBuf),
    Zip(RefCell<ZipArchive<File>>),
    ZipBytes(RefCell<ZipArchive<Cursor<Vec<u8>>>>),
}

impl AssetSource {
    pub fn new() -> Self {
        Self { layers: Vec::new() }
    }

    /// Append a base layer.
    pub fn add_dir(&mut self, path: &Path) -> Result<(), MaterialError> {
        if !path.is_dir() {
            return Err(MaterialError::read(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        self.layers.push(Layer::Dir(path.to_path_buf()));
        Ok(())
    }

    /// Append a base layer.
    pub fn add_jar(&mut self, path: &Path) -> Result<(), MaterialError> {
        self.layers.push(Layer::Zip(open_zip(path)?));
        Ok(())
    }

    /// Append an in-memory ZIP or jar.
    ///
    /// This is the entry point for hosts without a filesystem, such as
    /// Cloudflare Workers, where the resource pack arrives as bytes from an
    /// assets or object-storage binding.
    pub fn add_zip_bytes(&mut self, bytes: Vec<u8>) -> Result<(), MaterialError> {
        self.layers.push(Layer::ZipBytes(open_zip_bytes(bytes)?));
        Ok(())
    }

    /// Append an in-memory overlay ZIP or jar in front of the existing layers.
    pub fn add_overlay_zip_bytes(&mut self, bytes: Vec<u8>) -> Result<(), MaterialError> {
        self.layers
            .insert(0, Layer::ZipBytes(open_zip_bytes(bytes)?));
        Ok(())
    }

    /// Add an overlay directory in front of the existing layers.
    pub fn add_overlay_dir(&mut self, path: &Path) -> Result<(), MaterialError> {
        if !path.is_dir() {
            return Err(MaterialError::read(format!(
                "{} is not a directory",
                path.display()
            )));
        }
        self.layers.insert(0, Layer::Dir(path.to_path_buf()));
        Ok(())
    }

    /// Add an overlay zip or jar in front of the existing layers.
    pub fn add_overlay_zip(&mut self, path: &Path) -> Result<(), MaterialError> {
        self.layers.insert(0, Layer::Zip(open_zip(path)?));
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Read a normalized asset path such as
    /// `assets/minecraft/blockstates/stone.json`.
    pub fn read(&self, path: &str) -> Result<Option<Vec<u8>>, MaterialError> {
        let path = normalize_asset_path(path)
            .ok_or_else(|| MaterialError::read(format!("unsafe asset path '{path}'")))?;

        for layer in &self.layers {
            if let Some(bytes) = layer.read(&path, MAX_ASSET_BYTES)? {
                return Ok(Some(bytes));
            }
        }
        Ok(None)
    }
}

impl Default for AssetSource {
    fn default() -> Self {
        Self::new()
    }
}

impl Layer {
    fn read(&self, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, MaterialError> {
        match self {
            Self::Dir(root) => read_dir(root, path, max_bytes),
            Self::Zip(archive) => read_zip(archive, path, max_bytes),
            Self::ZipBytes(archive) => read_zip(archive, path, max_bytes),
        }
    }
}

fn read_dir(root: &Path, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, MaterialError> {
    for candidate in dir_candidates(root, path) {
        let metadata = match fs::metadata(&candidate) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        if !metadata.is_file() {
            continue;
        }

        // Check the size before opening so a large local asset cannot be read
        // into memory just to be rejected afterwards.
        if metadata.len() > max_bytes as u64 {
            return Err(MaterialError::limit(format!(
                "{} is {} bytes, limit is {max_bytes}",
                candidate.display(),
                metadata.len()
            )));
        }

        // Keep the `take` guard for a file that grows between metadata and read.
        let file = File::open(&candidate).map_err(|error| {
            MaterialError::read(format!("cannot read {}: {error}", candidate.display()))
        })?;
        let label = candidate.display().to_string();
        let bytes = read_reader_limited(file, max_bytes, &label, metadata.len() as usize)?;
        return Ok(Some(bytes));
    }
    Ok(None)
}

fn dir_candidates(root: &Path, path: &str) -> Vec<PathBuf> {
    let mut candidates = vec![root.join(path)];

    if let Some(rest) = path.strip_prefix("assets/") {
        candidates.push(root.join(rest));
    }
    if let Some(rest) = path.strip_prefix("assets/minecraft/") {
        candidates.push(root.join(rest));
        candidates.push(root.join("minecraft").join(rest));
    }

    candidates
}

fn read_zip<R: Read + Seek>(
    archive: &RefCell<ZipArchive<R>>,
    path: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, MaterialError> {
    let mut archive = archive.borrow_mut();

    for candidate in zip_candidates(path) {
        let entry = match archive.by_name(&candidate) {
            Ok(entry) => entry,
            Err(zip::result::ZipError::FileNotFound) => continue,
            Err(error) => {
                return Err(MaterialError::read(format!(
                    "cannot read zip entry {candidate}: {error}"
                )));
            }
        };

        if entry.size() > max_bytes as u64 {
            return Err(MaterialError::limit(format!(
                "zip entry {candidate} is {} bytes, limit is {max_bytes}",
                entry.size()
            )));
        }

        // The uncompressed size in the central directory is untrusted. Bound
        // decompression itself so a crafted zip cannot expand without limit.
        let capacity = entry.size() as usize;
        let label = format!("zip entry {candidate}");
        let bytes = read_reader_limited(entry, max_bytes, &label, capacity)?;
        return Ok(Some(bytes));
    }

    Ok(None)
}

/// Read a stream without buffering more than `max_bytes`.
///
/// The `take` guard is what bounds decompression; the caller's declared size
/// is only used as a capacity hint.
fn read_reader_limited<R: Read>(
    reader: R,
    max_bytes: usize,
    label: &str,
    capacity: usize,
) -> Result<Vec<u8>, MaterialError> {
    let mut bytes = Vec::with_capacity(capacity.min(max_bytes));
    reader
        .take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| MaterialError::read(format!("cannot read {label}: {error}")))?;
    if bytes.len() > max_bytes {
        return Err(MaterialError::limit(format!(
            "{label} expands beyond {max_bytes} bytes"
        )));
    }
    Ok(bytes)
}

fn zip_candidates(path: &str) -> Vec<String> {
    let mut candidates = vec![path.to_string()];
    if let Some(rest) = path.strip_prefix("assets/") {
        candidates.push(rest.to_string());
    }
    candidates
}

fn open_zip(path: &Path) -> Result<RefCell<ZipArchive<File>>, MaterialError> {
    let file = File::open(path)
        .map_err(|error| MaterialError::read(format!("cannot open {}: {error}", path.display())))?;
    let archive = ZipArchive::new(file).map_err(|error| {
        MaterialError::new(
            MaterialErrorCode::UnsupportedSource,
            format!("{} is not a readable jar/zip: {error}", path.display()),
        )
    })?;
    Ok(RefCell::new(archive))
}

fn open_zip_bytes(bytes: Vec<u8>) -> Result<RefCell<ZipArchive<Cursor<Vec<u8>>>>, MaterialError> {
    let archive = ZipArchive::new(Cursor::new(bytes)).map_err(|error| {
        MaterialError::new(
            MaterialErrorCode::UnsupportedSource,
            format!("resource pack is not a readable zip: {error}"),
        )
    })?;
    Ok(RefCell::new(archive))
}

fn normalize_asset_path(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(':')
        || path
            .split('/')
            .any(|part| part == ".." || part == "." || part.is_empty())
    {
        return None;
    }
    Some(path)
}

/// Build the normalized path for a namespaced texture, model, or blockstate.
pub fn asset_path(kind: &str, name: &str, extension: &str) -> String {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    format!("assets/minecraft/{kind}/{name}.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_unsafe_asset_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();

        assert!(source.read("assets/../secret").is_err());
        assert!(source.read("C:/secret").is_err());
        assert!(source.read("/absolute").is_err());
    }

    #[test]
    fn rejects_oversized_directory_assets_before_reading() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("large.bin"), vec![0u8; 64]).unwrap();

        let error = read_dir(dir.path(), "large.bin", 8).unwrap_err();
        assert_eq!(error.code, MaterialErrorCode::AssetLimit);
    }

    struct CountingReader {
        remaining: usize,
        consumed: usize,
    }

    impl std::io::Read for CountingReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = self.remaining.min(buffer.len());
            buffer[..count].fill(b'x');
            self.remaining -= count;
            self.consumed += count;
            Ok(count)
        }
    }

    #[test]
    fn bounds_reader_reads_to_the_limit_plus_one() {
        let mut reader = CountingReader {
            remaining: 64,
            consumed: 0,
        };

        let error = read_reader_limited(&mut reader, 8, "test", 64).unwrap_err();
        assert_eq!(error.code, MaterialErrorCode::AssetLimit);
        assert_eq!(
            reader.consumed, 9,
            "the reader must stop after limit + 1 bytes"
        );
    }

    #[test]
    fn accepts_a_reader_at_the_exact_limit() {
        let reader = CountingReader {
            remaining: 8,
            consumed: 0,
        };

        let bytes = read_reader_limited(reader, 8, "test", 8).unwrap();
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn reads_assets_from_an_in_memory_zip() {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut cursor);
            writer
                .start_file(
                    "assets/minecraft/blockstates/test.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(b"{\"variants\":{}}").unwrap();
            writer.finish().unwrap();
        }

        let mut source = AssetSource::new();
        source.add_zip_bytes(cursor.into_inner()).unwrap();
        let bytes = source
            .read("assets/minecraft/blockstates/test.json")
            .unwrap()
            .unwrap();
        assert_eq!(bytes, b"{\"variants\":{}}");
    }

    #[test]
    fn rejects_oversized_zip_assets_during_decompression() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.zip");
        {
            let file = File::create(&path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file(
                    "assets/minecraft/blockstates/big.json",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(&[b'x'; 64]).unwrap();
            writer.finish().unwrap();
        }

        let archive = open_zip(&path).unwrap();
        let error = read_zip(&archive, "assets/minecraft/blockstates/big.json", 8).unwrap_err();
        assert_eq!(error.code, MaterialErrorCode::AssetLimit);
    }
}
