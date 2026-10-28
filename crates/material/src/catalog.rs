use crate::blockstate::select_variants;
use crate::json::BlockstateFile;
use crate::model::{ModelChain, ResolvedTexture, build_model, resolve_model, resolve_texture};
use crate::source::AssetSource;
use crate::texture::TextureSet;
use crate::tint::tint_for;
use crate::{MaterialError, MaterialErrorCode};
use litematic_mesh::{
    BlockCatalog, BlockModel, ColorBlockCatalog, FaceDirection, NO_TEXTURE, RenderLayer,
    ResolvedBlock, Rgba, needs_water_style,
};
use litematic_scene::{BlockStateKey, Scene};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MaterialKind {
    PackModel,
    Chest,
    Fallback,
    Unknown,
}

impl MaterialKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackModel => "pack-model",
            Self::Chest => "chest",
            Self::Fallback => "fallback",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialInfo {
    pub kind: MaterialKind,
    pub textured: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDiagnostics {
    pub pack_models: u64,
    pub chests: u64,
    pub fallback_shapes: u64,
    pub missing_models: u64,
    pub missing_textures: u64,
    pub missing_blockstates: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialEntry {
    pub state: String,
    pub count: u64,
    pub kind: MaterialKind,
    pub textured: bool,
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialReport {
    pub total_blocks: u64,
    pub pack_models: u64,
    pub chests: u64,
    pub fallback: u64,
    pub unknown: u64,
    pub textured: u64,
    pub missing_textures: u64,
    pub entries: Vec<MaterialEntry>,
}

/// A resource-pack-backed catalog with the original color-block catalog as a
/// fallback for missing models and textures.
pub struct MaterialSet {
    source: AssetSource,
    fallback: ColorBlockCatalog,
    textures: RefCell<TextureSet>,
    blockstates: RefCell<HashMap<String, Option<Arc<BlockstateFile>>>>,
    models: RefCell<HashMap<String, Option<ModelChain>>>,
    resolved: RefCell<HashMap<BlockStateKey, Arc<ResolvedBlock>>>,
    info: RefCell<HashMap<BlockStateKey, MaterialInfo>>,
    diagnostics: RefCell<MaterialDiagnostics>,
}

impl MaterialSet {
    pub fn open(source: AssetSource) -> Result<Self, MaterialError> {
        let fallback = ColorBlockCatalog::new().map_err(|error| {
            MaterialError::new(MaterialErrorCode::ModelInvalid, error.to_string())
        })?;
        Ok(Self {
            source,
            fallback,
            textures: RefCell::new(TextureSet::new()),
            blockstates: RefCell::new(HashMap::new()),
            models: RefCell::new(HashMap::new()),
            resolved: RefCell::new(HashMap::new()),
            info: RefCell::new(HashMap::new()),
            diagnostics: RefCell::new(MaterialDiagnostics::default()),
        })
    }

    pub fn textures(&self) -> std::cell::Ref<'_, TextureSet> {
        self.textures.borrow()
    }

    pub fn diagnostics(&self) -> MaterialDiagnostics {
        let mut diagnostics = self.diagnostics.borrow().clone();
        diagnostics.missing_textures = self.textures.borrow().missing();
        diagnostics
    }

    pub fn info_for(&self, state: &BlockStateKey) -> Option<MaterialInfo> {
        self.info.borrow().get(state).copied()
    }

    pub fn report(&self, scene: &Scene) -> MaterialReport {
        let mut entries = Vec::new();
        let mut total_blocks = 0u64;
        let mut pack_models = 0u64;
        let mut chests = 0u64;
        let mut fallback = 0u64;
        let mut unknown = 0u64;
        let mut textured = 0u64;

        for (state, count) in &scene.statistics.blocks {
            if state.is_air() {
                continue;
            }
            let resolved = self.resolve(state);
            let info = self.info_for(state).unwrap_or(MaterialInfo {
                kind: MaterialKind::Fallback,
                textured: false,
            });

            total_blocks += count;
            match info.kind {
                MaterialKind::PackModel => pack_models += count,
                MaterialKind::Chest => chests += count,
                MaterialKind::Fallback => fallback += count,
                MaterialKind::Unknown => unknown += count,
            }
            if info.textured {
                textured += count;
            }

            entries.push(MaterialEntry {
                state: state.canonical(),
                count: *count,
                kind: info.kind,
                textured: info.textured,
                known: resolved.known,
            });
        }

        MaterialReport {
            total_blocks,
            pack_models,
            chests,
            fallback,
            unknown,
            textured,
            missing_textures: self.textures.borrow().missing(),
            entries,
        }
    }

    fn resolve_uncached(&self, state: &BlockStateKey) -> ResolvedBlock {
        let fallback = self.fallback.resolve(state);

        if state.is_air() {
            return fallback;
        }

        let pack = self.pack_model(state);

        if let Some(model) =
            crate::chest::build_chest_model(state, &self.source, &mut self.textures.borrow_mut())
        {
            self.diagnostics.borrow_mut().chests += 1;
            self.info.borrow_mut().insert(
                state.clone(),
                MaterialInfo {
                    kind: MaterialKind::Chest,
                    textured: true,
                },
            );
            return ResolvedBlock {
                color: fallback.color,
                layer: RenderLayer::Opaque,
                model: Arc::new(model),
                known: fallback.known,
            };
        }

        if let Some(pack) = pack {
            if let Some(mut model) = pack.model {
                if fallback.layer == RenderLayer::Opaque {
                    let textures = self.textures.borrow();
                    for quad in &mut model.quads {
                        if quad.layer.is_none()
                            && quad.texture != NO_TEXTURE
                            && textures.has_transparency(quad.texture)
                        {
                            quad.layer = Some(RenderLayer::Cutout);
                        }
                    }
                }

                self.diagnostics.borrow_mut().pack_models += 1;
                let textured = model.quads.iter().any(|quad| quad.texture != NO_TEXTURE);
                self.info.borrow_mut().insert(
                    state.clone(),
                    MaterialInfo {
                        kind: MaterialKind::PackModel,
                        textured,
                    },
                );
                return ResolvedBlock {
                    color: fallback.color,
                    layer: fallback.layer,
                    model: Arc::new(model),
                    known: fallback.known,
                };
            }

            let (model, textured) =
                self.fallback_model(state, fallback.model.clone(), pack.particle, fallback.layer);
            self.diagnostics.borrow_mut().fallback_shapes += 1;
            self.info.borrow_mut().insert(
                state.clone(),
                MaterialInfo {
                    kind: MaterialKind::Fallback,
                    textured,
                },
            );
            return ResolvedBlock {
                color: fallback.color,
                layer: fallback.layer,
                model: Arc::new(model),
                known: fallback.known,
            };
        }

        let (model, textured) =
            self.fallback_model(state, fallback.model.clone(), None, fallback.layer);
        self.diagnostics.borrow_mut().fallback_shapes += 1;
        self.info.borrow_mut().insert(
            state.clone(),
            MaterialInfo {
                kind: if fallback.known {
                    MaterialKind::Fallback
                } else {
                    MaterialKind::Unknown
                },
                textured,
            },
        );
        ResolvedBlock {
            color: fallback.color,
            layer: fallback.layer,
            model: Arc::new(model),
            known: fallback.known,
        }
    }

    fn pack_model(&self, state: &BlockStateKey) -> Option<PackResolution> {
        let file = self.blockstate(&state.name)?;
        let variants = select_variants(&file, state);
        if variants.is_empty() {
            self.diagnostics.borrow_mut().missing_models += 1;
            return None;
        }

        let mut quads = Vec::new();
        let mut ambient_occlusion = true;
        let mut full_cube = variants.len() == 1;
        let mut particle = None;

        for variant in variants {
            let chain = {
                let mut models = self.models.borrow_mut();
                let mut stack = Vec::new();
                resolve_model(&self.source, &variant.model, &mut models, &mut stack)
            };
            let Some(chain) = chain else {
                self.diagnostics.borrow_mut().missing_models += 1;
                continue;
            };

            particle = particle.or_else(|| resolve_texture("#particle", &chain.textures));

            let mut textures = self.textures.borrow_mut();
            let Some(model) = build_model(
                &chain,
                state,
                variant.x,
                variant.y,
                variant.uvlock,
                &self.source,
                &mut textures,
            ) else {
                continue;
            };

            full_cube &= model.is_full_cube;
            ambient_occlusion &= model.ambient_occlusion;
            quads.extend(model.quads);
        }

        if quads.is_empty() {
            return Some(PackResolution {
                model: None,
                particle,
            });
        }

        Some(PackResolution {
            model: Some(BlockModel {
                quads,
                is_full_cube: full_cube,
                ambient_occlusion,
            }),
            particle,
        })
    }

    fn blockstate(&self, name: &str) -> Option<Arc<BlockstateFile>> {
        let path = name.strip_prefix("minecraft:").unwrap_or(name).to_string();

        if let Some(cached) = self.blockstates.borrow().get(&path) {
            return cached.clone();
        }

        let asset = format!("assets/minecraft/blockstates/{path}.json");
        let parsed = match self.source.read(&asset) {
            Ok(Some(bytes)) => serde_json::from_slice::<BlockstateFile>(&bytes)
                .ok()
                .map(Arc::new),
            Ok(None) => {
                self.diagnostics.borrow_mut().missing_blockstates += 1;
                None
            }
            Err(_) => None,
        };

        self.blockstates.borrow_mut().insert(path, parsed.clone());
        parsed
    }

    fn fallback_model(
        &self,
        state: &BlockStateKey,
        model: Arc<BlockModel>,
        particle: Option<ResolvedTexture>,
        base_layer: RenderLayer,
    ) -> (BlockModel, bool) {
        let mut model = (*model).clone();
        let mut textured = false;
        let mut textures = self.textures.borrow_mut();

        for quad in &mut model.quads {
            let name = particle
                .as_ref()
                .map(|texture| texture.path.clone())
                .or_else(|| fallback_texture_name(state, quad.face));

            let Some(name) = name else {
                continue;
            };
            let Some(id) = textures.load(&self.source, &name) else {
                continue;
            };

            quad.texture = id;
            quad.tint = fallback_tint(state, quad.face);
            if base_layer == RenderLayer::Opaque && textures.has_transparency(id) {
                quad.layer = Some(RenderLayer::Cutout);
            }
            textured = true;
        }

        (model, textured)
    }
}

/// Immutable material state that can be shared across threads.
///
/// Building a snapshot resolves every requested state once, so mesh building
/// and rendering only perform lookups. `resolve` never writes and never locks.
pub struct MaterialSnapshot {
    resolved: HashMap<BlockStateKey, ResolvedBlock>,
    info: HashMap<BlockStateKey, MaterialInfo>,
    textures: TextureSet,
    diagnostics: MaterialDiagnostics,
    air: ResolvedBlock,
    missing: ResolvedBlock,
}

impl MaterialSnapshot {
    pub fn len(&self) -> usize {
        self.resolved.len()
    }

    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty()
    }

    pub fn textures(&self) -> &TextureSet {
        &self.textures
    }

    pub fn diagnostics(&self) -> &MaterialDiagnostics {
        &self.diagnostics
    }

    pub fn info_for(&self, state: &BlockStateKey) -> Option<MaterialInfo> {
        self.info.get(state).copied()
    }
}

impl BlockCatalog for MaterialSnapshot {
    fn resolve(&self, state: &BlockStateKey) -> ResolvedBlock {
        if state.is_air() {
            return self.air.clone();
        }

        match self.resolved.get(state) {
            Some(block) => block.clone(),
            None => self.missing.clone(),
        }
    }
}

impl MaterialSet {
    /// Pre-resolve `states` into an immutable snapshot.
    ///
    /// Air states are not stored; they resolve to an empty model. States that
    /// were not part of the snapshot resolve to a magenta unknown block, so a
    /// forgotten state stays visible instead of silently disappearing.
    pub fn snapshot<I>(&self, states: I) -> MaterialSnapshot
    where
        I: IntoIterator<Item = BlockStateKey>,
    {
        let mut resolved = HashMap::new();
        let mut info = HashMap::new();
        let mut needs_water = false;

        for state in states {
            if state.is_air() || resolved.contains_key(&state) {
                continue;
            }

            needs_water |= needs_water_style(&state);
            let block = self.resolve(&state);
            if let Some(entry) = self.info_for(&state) {
                info.insert(state.clone(), entry);
            }
            resolved.insert(state, block);
        }

        // Waterlogged blocks need a water style even when the scene has no
        // standalone water cell.
        if needs_water {
            let water = BlockStateKey::simple("minecraft:water");
            resolved
                .entry(water.clone())
                .or_insert_with(|| self.resolve(&water));
        }

        let textures = self.textures.borrow().clone();
        let diagnostics = self.diagnostics();
        let air = self
            .fallback
            .resolve(&BlockStateKey::simple("minecraft:air"));
        let missing = self
            .fallback
            .resolve(&BlockStateKey::simple("minecraft:__unresolved__"));

        MaterialSnapshot {
            resolved,
            info,
            textures,
            diagnostics,
            air,
            missing,
        }
    }
}

impl BlockCatalog for MaterialSet {
    fn resolve(&self, state: &BlockStateKey) -> ResolvedBlock {
        if let Some(cached) = self.resolved.borrow().get(state) {
            return (**cached).clone();
        }
        let resolved = self.resolve_uncached(state);
        self.resolved
            .borrow_mut()
            .insert(state.clone(), Arc::new(resolved.clone()));
        resolved
    }
}

struct PackResolution {
    model: Option<BlockModel>,
    particle: Option<ResolvedTexture>,
}

fn fallback_texture_name(state: &BlockStateKey, face: FaceDirection) -> Option<String> {
    let path = state
        .name
        .split_once(':')
        .map(|(_, path)| path)
        .unwrap_or(&state.name);

    if path == "water" || path == "flowing_water" || path == "bubble_column" {
        return Some("block/water_still".to_string());
    }
    if path == "lava" || path == "flowing_lava" {
        return Some("block/lava_still".to_string());
    }
    if path == "grass_block" {
        return Some(match face {
            FaceDirection::Up => "block/grass_block_top".to_string(),
            FaceDirection::Down => "block/dirt".to_string(),
            _ => "block/grass_block_side".to_string(),
        });
    }
    if path.ends_with("_log") {
        return Some(match face {
            FaceDirection::Up | FaceDirection::Down => format!("block/{path}_top"),
            _ => format!("block/{path}"),
        });
    }
    if path.ends_with("_wood") {
        let base = path.trim_end_matches("_wood");
        return Some(match face {
            FaceDirection::Up | FaceDirection::Down => format!("block/{base}_log_top"),
            _ => format!("block/{path}"),
        });
    }
    if path == "cactus" {
        return Some(match face {
            FaceDirection::Up => "block/cactus_top".to_string(),
            FaceDirection::Down => "block/cactus_bottom".to_string(),
            _ => "block/cactus_side".to_string(),
        });
    }
    if path == "target" {
        return Some(match face {
            FaceDirection::Up | FaceDirection::Down => "block/target_top".to_string(),
            _ => "block/target_side".to_string(),
        });
    }
    if path == "composter" {
        return Some(match face {
            FaceDirection::Up => "block/composter_top".to_string(),
            FaceDirection::Down => "block/composter_bottom".to_string(),
            _ => "block/composter_side".to_string(),
        });
    }
    if path == "barrel" {
        return Some(match face {
            FaceDirection::Up | FaceDirection::Down => "block/barrel_top".to_string(),
            _ => "block/barrel_side".to_string(),
        });
    }
    if path.ends_with("_wall_torch") {
        return Some(format!("block/{}", path.replace("_wall_torch", "_torch")));
    }
    if path.ends_with("_coral_wall_fan") {
        return Some(format!("block/{}", path.replace("_wall_fan", "_fan")));
    }
    if path == "redstone_wire" {
        return Some("block/redstone_dust_dot".to_string());
    }
    if path == "repeater" || path == "comparator" {
        return Some(format!("block/{path}"));
    }
    if path.ends_with("_pane") {
        if path == "glass_pane" {
            return Some("block/glass".to_string());
        }
        return Some(format!("block/{}", path.replace("_pane", "")));
    }
    if path.ends_with("_carpet") {
        if path == "moss_carpet" {
            return Some("block/moss_block".to_string());
        }
        return Some(format!("block/{}", path.replace("_carpet", "_wool")));
    }
    if path.ends_with("_fence") || path.ends_with("_fence_gate") {
        let base = path
            .trim_end_matches("_fence_gate")
            .trim_end_matches("_fence");
        if is_wood(base) {
            return Some(format!("block/{base}_planks"));
        }
        if base == "nether_brick" {
            return Some("block/nether_bricks".to_string());
        }
        return Some(format!("block/{base}"));
    }
    if path.ends_with("_wall") || path.ends_with("_stairs") || path.ends_with("_slab") {
        let base = path
            .trim_end_matches("_wall")
            .trim_end_matches("_stairs")
            .trim_end_matches("_slab");
        if is_wood(base) {
            return Some(format!("block/{base}_planks"));
        }
        if base.ends_with("_brick") {
            return Some(format!("block/{base}s"));
        }
        return Some(format!("block/{base}"));
    }
    if path.ends_with("_button") || path.ends_with("_pressure_plate") {
        let base = path
            .trim_end_matches("_button")
            .trim_end_matches("_pressure_plate");
        if is_wood(base) {
            return Some(format!("block/{base}_planks"));
        }
        return Some(format!("block/{base}"));
    }

    Some(format!("block/{path}"))
}

fn fallback_tint(state: &BlockStateKey, face: FaceDirection) -> Rgba {
    let path = state
        .name
        .split_once(':')
        .map(|(_, path)| path)
        .unwrap_or(&state.name);

    if path == "grass_block" && face == FaceDirection::Up {
        return tint_for(state, Some(0));
    }
    if path == "grass_block" {
        return Rgba::WHITE;
    }
    if path == "water"
        || path == "flowing_water"
        || path == "bubble_column"
        || path == "redstone_wire"
        || path.contains("leaves")
    {
        return tint_for(state, Some(0));
    }
    if path == "short_grass" || path == "tall_grass" || path == "fern" || path == "large_fern" {
        return tint_for(state, Some(0));
    }

    Rgba::WHITE
}

fn is_wood(name: &str) -> bool {
    matches!(
        name,
        "oak"
            | "spruce"
            | "birch"
            | "jungle"
            | "acacia"
            | "dark_oak"
            | "mangrove"
            | "cherry"
            | "bamboo"
            | "crimson"
            | "warped"
            | "pale_oak"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_texture_rules_cover_common_shapes() {
        let state = BlockStateKey::simple("minecraft:oak_stairs");
        assert_eq!(
            fallback_texture_name(&state, FaceDirection::Up).as_deref(),
            Some("block/oak_planks")
        );

        let state = BlockStateKey::simple("minecraft:stone_brick_wall");
        assert_eq!(
            fallback_texture_name(&state, FaceDirection::North).as_deref(),
            Some("block/stone_bricks")
        );

        let state = BlockStateKey::simple("minecraft:white_stained_glass_pane");
        assert_eq!(
            fallback_texture_name(&state, FaceDirection::North).as_deref(),
            Some("block/white_stained_glass")
        );
    }
}
