use crate::source::AssetSource;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

const MAX_TEXTURE_SIZE: u32 = 2048;
const MAX_TEXTURE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextureFilter {
    Nearest,
    Linear,
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Texture {
    /// Sample one texel or a bilinear blend. UVs repeat outside `0..1`.
    #[inline]
    pub fn sample(&self, uv: [f32; 2], filter: TextureFilter) -> Option<[f32; 4]> {
        if self.width == 0 || self.height == 0 {
            return None;
        }

        // Nearest sampling dominates the CPU path, and almost all model UVs
        // stay inside 0..1. Skip the repeat/rem_euclid work in that common
        // case while keeping the exact rounding of the general path.
        if filter == TextureFilter::Nearest
            && (0.0..1.0).contains(&uv[0])
            && (0.0..1.0).contains(&uv[1])
        {
            let width = self.width as usize;
            let height = self.height as usize;
            let x = (uv[0] * self.width as f32) as usize;
            let y = (uv[1] * self.height as f32) as usize;
            let x = if x >= width { 0 } else { x };
            let y = if y >= height { 0 } else { y };
            let index = (y * width + x) * 4;
            const INV_255: f32 = 1.0 / 255.0;
            return Some([
                f32::from(self.rgba[index]) * INV_255,
                f32::from(self.rgba[index + 1]) * INV_255,
                f32::from(self.rgba[index + 2]) * INV_255,
                f32::from(self.rgba[index + 3]) * INV_255,
            ]);
        }

        let x = repeat(uv[0]) * self.width as f32;
        let y = repeat(uv[1]) * self.height as f32;
        Some(match filter {
            TextureFilter::Nearest => texel(self, x.floor() as i32, y.floor() as i32),
            TextureFilter::Linear => bilinear(self, x - 0.5, y - 0.5),
        })
    }
}

#[derive(Debug)]
pub struct TextureSet {
    /// Shared so cloning a set for a material snapshot only bumps refcounts.
    textures: Vec<Arc<Texture>>,
    /// Per-texture alpha flag, computed once when the texture is decoded.
    transparency: Vec<bool>,
    lookup: HashMap<String, Option<u16>>,
    bytes: u64,
    missing: u64,
    /// Identity token for process-wide GPU resource caches. A clone gets a new
    /// token because it can be mutated independently of the source set.
    identity: Arc<()>,
}

impl Default for TextureSet {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for TextureSet {
    fn clone(&self) -> Self {
        Self {
            textures: self.textures.clone(),
            transparency: self.transparency.clone(),
            lookup: self.lookup.clone(),
            bytes: self.bytes,
            missing: self.missing,
            identity: Arc::new(()),
        }
    }
}

impl TextureSet {
    pub fn new() -> Self {
        Self {
            textures: Vec::new(),
            transparency: Vec::new(),
            lookup: HashMap::new(),
            bytes: 0,
            missing: 0,
            identity: Arc::new(()),
        }
    }

    /// Weak identity used to key process-wide GPU caches without keeping the
    /// texture set alive.
    pub fn identity(&self) -> std::sync::Weak<()> {
        Arc::downgrade(&self.identity)
    }

    pub fn len(&self) -> usize {
        self.textures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    pub fn missing(&self) -> u64 {
        self.missing
    }

    /// Decoded texture bytes currently held by this set.
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Load a texture path such as `block/stone` or `minecraft:block/stone`.
    /// Returns `None` when the asset is missing or cannot be decoded.
    pub fn load(&mut self, source: &AssetSource, path: &str) -> Option<u16> {
        let path = normalize_texture_path(path);
        if let Some(cached) = self.lookup.get(&path) {
            return *cached;
        }

        let id = self.load_uncached(source, &path);
        self.lookup.insert(path, id);
        if id.is_none() {
            self.missing += 1;
        }
        id
    }

    fn load_uncached(&mut self, source: &AssetSource, path: &str) -> Option<u16> {
        let asset = format!("assets/minecraft/textures/{path}.png");
        let bytes = source.read(&asset).ok().flatten()?;
        let animated = source
            .read(&format!("{asset}.mcmeta"))
            .ok()
            .flatten()
            .is_some_and(|bytes| has_animation(&bytes));

        let (width, height, rgba) = decode_png(&bytes, animated)?;
        let size = u64::from(width) * u64::from(height) * 4;
        if self.bytes.saturating_add(size) > MAX_TEXTURE_BYTES {
            return None;
        }
        self.bytes += size;

        let id = u16::try_from(self.textures.len()).ok()?;
        let has_alpha = rgba.as_chunks::<4>().0.iter().any(|pixel| pixel[3] < 255);
        self.textures.push(Arc::new(Texture {
            width,
            height,
            rgba,
        }));
        self.transparency.push(has_alpha);
        // Cached GPU resources key on this token; a texture added after a cache
        // entry exists must not keep serving the old atlas.
        self.identity = Arc::new(());
        Some(id)
    }

    pub fn get(&self, id: u16) -> Option<&Texture> {
        self.textures.get(usize::from(id)).map(Arc::as_ref)
    }

    pub fn has_transparency(&self, id: u16) -> bool {
        self.transparency
            .get(usize::from(id))
            .copied()
            .unwrap_or(false)
    }

    #[inline]
    pub fn sample(&self, id: u16, uv: [f32; 2], filter: TextureFilter) -> Option<[f32; 4]> {
        self.get(id)?.sample(uv, filter)
    }
}

fn normalize_texture_path(path: &str) -> String {
    let path = path.strip_prefix("minecraft:").unwrap_or(path);
    path.trim_start_matches("textures/").to_string()
}

#[inline]
fn repeat(value: f32) -> f32 {
    if (0.0..1.0).contains(&value) {
        value
    } else {
        value.rem_euclid(1.0)
    }
}

#[inline]
fn texel(texture: &Texture, x: i32, y: i32) -> [f32; 4] {
    let width = texture.width as i32;
    let height = texture.height as i32;
    let x = x.rem_euclid(width) as usize;
    let y = y.rem_euclid(height) as usize;
    let index = (y * texture.width as usize + x) * 4;
    const INV_255: f32 = 1.0 / 255.0;
    [
        f32::from(texture.rgba[index]) * INV_255,
        f32::from(texture.rgba[index + 1]) * INV_255,
        f32::from(texture.rgba[index + 2]) * INV_255,
        f32::from(texture.rgba[index + 3]) * INV_255,
    ]
}

#[inline]
fn bilinear(texture: &Texture, x: f32, y: f32) -> [f32; 4] {
    let x0 = x.floor();
    let y0 = y.floor();
    let tx = x - x0;
    let ty = y - y0;
    let x0 = x0 as i32;
    let y0 = y0 as i32;

    let c00 = texel(texture, x0, y0);
    let c10 = texel(texture, x0 + 1, y0);
    let c01 = texel(texture, x0, y0 + 1);
    let c11 = texel(texture, x0 + 1, y0 + 1);

    let mut out = [0.0f32; 4];
    for channel in 0..4 {
        let top = c00[channel] + (c10[channel] - c00[channel]) * tx;
        let bottom = c01[channel] + (c11[channel] - c01[channel]) * tx;
        out[channel] = top + (bottom - top) * ty;
    }
    out
}

fn decode_png(bytes: &[u8], animated: bool) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;

    let width = reader.info().width;
    let source_height = reader.info().height;
    if width == 0
        || source_height == 0
        || width > MAX_TEXTURE_SIZE
        || source_height > MAX_TEXTURE_SIZE
    {
        return None;
    }

    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let source = &buffer[..info.buffer_size()];
    let rgba = convert_to_rgba(source, width, source_height, info.color_type)?;

    let frame_height = if animated {
        width.min(source_height)
    } else {
        source_height
    };
    let cropped = crop_first_frame(&rgba, width, source_height, frame_height);
    Some((width, frame_height, cropped))
}

fn crop_first_frame(rgba: &[u8], width: u32, source_height: u32, frame_height: u32) -> Vec<u8> {
    if frame_height == source_height {
        return rgba.to_vec();
    }

    let row_bytes = width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * frame_height as usize);
    out.extend_from_slice(&rgba[..row_bytes * frame_height as usize]);
    out
}

fn convert_to_rgba(
    source: &[u8],
    width: u32,
    height: u32,
    color: png::ColorType,
) -> Option<Vec<u8>> {
    let pixels = width as usize * height as usize;
    match color {
        png::ColorType::Rgba => Some(source.get(..pixels * 4)?.to_vec()),
        png::ColorType::Rgb => {
            let source = source.get(..pixels * 3)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for chunk in source.as_chunks::<3>().0 {
                out.extend_from_slice(chunk);
                out.push(255);
            }
            Some(out)
        }
        png::ColorType::GrayscaleAlpha => {
            let source = source.get(..pixels * 2)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for chunk in source.as_chunks::<2>().0 {
                out.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            Some(out)
        }
        png::ColorType::Grayscale => {
            let source = source.get(..pixels)?;
            let mut out = Vec::with_capacity(pixels * 4);
            for value in source {
                out.extend_from_slice(&[*value, *value, *value, 255]);
            }
            Some(out)
        }
        png::ColorType::Indexed => None,
    }
}

fn has_animation(bytes: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(bytes)
        .ok()
        .and_then(|json| json.get("animation").cloned())
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::BufWriter;
    use std::path::Path;

    fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let file = File::create(path).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(rgba).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn animated_texture_uses_the_first_frame() {
        let dir = tempfile::tempdir().unwrap();
        let texture = dir.path().join("assets/minecraft/textures/block/test.png");
        let mut rgba = Vec::new();
        for _ in 0..16 {
            rgba.extend(std::iter::repeat_n([255u8, 0, 0, 255], 16).flatten());
        }
        for _ in 0..16 {
            rgba.extend(std::iter::repeat_n([0u8, 255, 0, 255], 16).flatten());
        }
        write_png(&texture, 16, 32, &rgba);
        fs::write(
            texture.with_extension("png.mcmeta"),
            r#"{"animation":{"frametime":2}}"#,
        )
        .unwrap();

        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();
        let mut textures = TextureSet::new();
        let id = textures.load(&source, "block/test").unwrap();
        assert_eq!(textures.get(id).unwrap().height, 16);

        let sample = textures
            .sample(id, [0.5, 0.5], TextureFilter::Nearest)
            .unwrap();
        assert!(sample[0] > 0.9 && sample[1] < 0.1);
    }

    #[test]
    fn nearest_sampling_handles_in_range_edges() {
        let texture = Texture {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, // red
                0, 255, 0, 255, // green
                0, 0, 255, 255, // blue
                255, 255, 255, 255, // white
            ],
        };

        assert_eq!(
            texture.sample([0.0, 0.0], TextureFilter::Nearest).unwrap(),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            texture
                .sample([0.999_999, 0.999_999], TextureFilter::Nearest)
                .unwrap(),
            [1.0, 1.0, 1.0, 1.0]
        );
    }

    #[test]
    fn texture_sampling_repeats_and_blends() {
        let texture = Texture {
            width: 2,
            height: 2,
            rgba: vec![
                255, 0, 0, 255, // red
                0, 255, 0, 255, // green
                0, 0, 255, 255, // blue
                255, 255, 255, 255, // white
            ],
        };

        assert_eq!(
            texture.sample([1.0, 0.0], TextureFilter::Nearest).unwrap(),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            texture
                .sample([0.75, 0.25], TextureFilter::Nearest)
                .unwrap(),
            [0.0, 1.0, 0.0, 1.0]
        );
        let blended = texture.sample([0.5, 0.5], TextureFilter::Linear).unwrap();
        assert!((blended[0] - 0.5).abs() < 1e-6);
        assert!((blended[1] - 0.5).abs() < 1e-6);
        assert!((blended[2] - 0.5).abs() < 1e-6);
        assert!((blended[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clone_shares_texture_storage() {
        let dir = tempfile::tempdir().unwrap();
        write_png(
            &dir.path()
                .join("assets/minecraft/textures/block/shared.png"),
            1,
            1,
            &[255, 0, 0, 255],
        );
        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();
        let mut textures = TextureSet::new();
        let id = textures.load(&source, "block/shared").unwrap();
        let original = textures.get(id).unwrap() as *const Texture;

        let cloned = textures.clone();

        assert_eq!(
            cloned.get(id).unwrap() as *const Texture,
            original,
            "cloning a texture set must not copy texture storage"
        );
        assert_eq!(cloned.bytes(), textures.bytes());
        assert!(
            !std::sync::Weak::ptr_eq(&textures.identity(), &cloned.identity()),
            "a mutable clone must get a distinct cache identity"
        );
    }

    #[test]
    fn loading_a_texture_changes_identity() {
        let dir = tempfile::tempdir().unwrap();
        let textures_dir = dir.path().join("assets/minecraft/textures/block");
        write_png(&textures_dir.join("first.png"), 1, 1, &[255, 0, 0, 255]);
        write_png(&textures_dir.join("second.png"), 1, 1, &[0, 255, 0, 255]);
        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();
        let mut textures = TextureSet::new();
        textures.load(&source, "block/first").unwrap();
        let first = textures.identity();

        textures.load(&source, "block/second").unwrap();
        let second = textures.identity();

        assert!(
            !std::sync::Weak::ptr_eq(&first, &second),
            "adding a texture must invalidate the cache identity"
        );
    }

    #[test]
    fn transparency_is_detected_from_decoded_pixels() {
        let dir = tempfile::tempdir().unwrap();
        write_png(
            &dir.path()
                .join("assets/minecraft/textures/block/opaque.png"),
            2,
            2,
            &[255u8; 16],
        );
        write_png(
            &dir.path().join("assets/minecraft/textures/block/alpha.png"),
            2,
            2,
            &[
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 128, 255, 255, 255, 255,
            ],
        );

        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();
        let mut textures = TextureSet::new();
        let opaque = textures.load(&source, "block/opaque").unwrap();
        let alpha = textures.load(&source, "block/alpha").unwrap();

        assert!(!textures.has_transparency(opaque));
        assert!(textures.has_transparency(alpha));
        assert!(!textures.has_transparency(u16::MAX));
    }

    #[test]
    fn missing_texture_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        let mut source = AssetSource::new();
        source.add_dir(dir.path()).unwrap();
        let mut textures = TextureSet::new();
        assert!(textures.load(&source, "block/missing").is_none());
        assert_eq!(textures.missing(), 1);
    }
}
