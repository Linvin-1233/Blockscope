use crate::{MaterialError, MaterialErrorCode};
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::Read;
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
        }
    }
}

fn read_dir(root: &Path, path: &str, max_bytes: usize) -> Result<Option<Vec<u8>>, MaterialError> {
    for candidate in dir_candidates(root, path) {
        if !candidate.is_file() {
            continue;
        }
        let bytes = fs::read(&candidate).map_err(|error| {
            MaterialError::read(format!("cannot read {}: {error}", candidate.display()))
        })?;
        if bytes.len() > max_bytes {
            return Err(MaterialError::limit(format!(
                "{} is {} bytes, limit is {max_bytes}",
                candidate.display(),
                bytes.len()
            )));
        }
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

fn read_zip(
    archive: &RefCell<ZipArchive<File>>,
    path: &str,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, MaterialError> {
    let mut archive = archive.borrow_mut();

    for candidate in zip_candidates(path) {
        let mut entry = match archive.by_name(&candidate) {
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

        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| MaterialError::read(format!("cannot read {candidate}: {error}")))?;
        if bytes.len() > max_bytes {
            return Err(MaterialError::limit(format!(
                "zip entry {candidate} expands beyond {max_bytes} bytes"
            )));
        }
        return Ok(Some(bytes));
    }

    Ok(None)
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

    #[test]
    fn rejects_unsafe_asset_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();

        assert!(source.read("assets/../secret").is_err());
        assert!(source.read("C:/secret").is_err());
        assert!(source.read("/absolute").is_err());
    }
}
