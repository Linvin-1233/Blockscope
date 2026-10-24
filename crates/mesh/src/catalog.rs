use crate::color::{RenderLayer, Rgba};
use crate::model::{BlockModel, empty_model, full_cube_model};
use crate::shape;
use litematic_scene::BlockStateKey;
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

/// A block resolved to color, layer, and model.
#[derive(Debug, Clone)]
pub struct ResolvedBlock {
    pub color: Rgba,
    pub layer: RenderLayer,
    pub model: Arc<BlockModel>,
    pub known: bool,
}

impl ResolvedBlock {
    pub fn occludes(&self) -> bool {
        self.model.occludes(self.layer)
    }
}

/// Resolves a canonical block state to a renderable model.
pub trait BlockCatalog {
    fn resolve(&self, state: &BlockStateKey) -> ResolvedBlock;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogErrorCode {
    InvalidJson,
    InvalidColor,
    InvalidLayer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError {
    pub code: CatalogErrorCode,
    pub message: String,
}

impl CatalogError {
    fn new(code: CatalogErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CatalogError {}

#[derive(Debug, Deserialize)]
struct PaletteFile {
    blocks: BTreeMap<String, PaletteEntry>,
}

#[derive(Debug, Deserialize)]
struct PaletteEntry {
    color: String,
    layer: Option<String>,
}

#[derive(Debug, Clone)]
struct PaletteEntryData {
    color: Rgba,
    layer: Option<RenderLayer>,
}

/// Color-block catalog backed by an embedded original palette.
#[derive(Debug, Clone)]
pub struct ColorBlockCatalog {
    entries: BTreeMap<String, PaletteEntryData>,
    model_cache: RefCell<HashMap<BlockStateKey, Arc<BlockModel>>>,
}

impl ColorBlockCatalog {
    pub fn new() -> Result<Self, CatalogError> {
        Self::from_json_str(include_str!("../data/color-palette.json"))
    }

    pub fn from_json_str(json: &str) -> Result<Self, CatalogError> {
        let file: PaletteFile = serde_json::from_str(json)
            .map_err(|error| CatalogError::new(CatalogErrorCode::InvalidJson, error.to_string()))?;

        let mut entries = BTreeMap::new();
        for (name, entry) in file.blocks {
            let color = Rgba::from_hex(&entry.color).map_err(|error| {
                CatalogError::new(CatalogErrorCode::InvalidColor, format!("{name}: {error}"))
            })?;
            let layer = match entry.layer.as_deref() {
                None => None,
                Some("opaque") => Some(RenderLayer::Opaque),
                Some("cutout") => Some(RenderLayer::Cutout),
                Some("translucent") => Some(RenderLayer::Translucent),
                Some(other) => {
                    return Err(CatalogError::new(
                        CatalogErrorCode::InvalidLayer,
                        format!("{name}: unknown layer '{other}'"),
                    ));
                }
            };
            entries.insert(name, PaletteEntryData { color, layer });
        }

        Ok(Self {
            entries,
            model_cache: RefCell::new(HashMap::new()),
        })
    }

    fn model_for(&self, state: &BlockStateKey) -> Arc<BlockModel> {
        let cached = self.model_cache.borrow().get(state).cloned();
        if let Some(model) = cached {
            return model;
        }

        let model = match shape::model_for(state) {
            Some(model) => Arc::new(model),
            None => full_cube_model(),
        };
        self.model_cache
            .borrow_mut()
            .insert(state.clone(), Arc::clone(&model));
        model
    }
}

impl Default for ColorBlockCatalog {
    fn default() -> Self {
        Self::new().expect("embedded color palette must be valid")
    }
}

impl BlockCatalog for ColorBlockCatalog {
    fn resolve(&self, state: &BlockStateKey) -> ResolvedBlock {
        if state.is_air() {
            return ResolvedBlock {
                color: Rgba::TRANSPARENT,
                layer: RenderLayer::Cutout,
                model: empty_model(),
                known: true,
            };
        }

        if let Some(entry) = self.entries.get(&state.name) {
            return ResolvedBlock {
                color: entry.color,
                layer: entry.layer.unwrap_or_else(|| layer_for_name(&state.name)),
                model: self.model_for(state),
                known: true,
            };
        }

        if let Some((color, layer)) = resolve_family(&state.name, self) {
            return ResolvedBlock {
                color,
                layer,
                model: self.model_for(state),
                known: true,
            };
        }

        ResolvedBlock {
            color: Rgba::MAGENTA,
            layer: RenderLayer::Opaque,
            model: self.model_for(state),
            known: false,
        }
    }
}

fn resolve_family(name: &str, catalog: &ColorBlockCatalog) -> Option<(Rgba, RenderLayer)> {
    const SUFFIXES: &[&str] = &[
        "_stairs",
        "_slab",
        "_wall",
        "_button",
        "_pressure_plate",
        "_fence_gate",
        "_fence",
        "_door",
        "_trapdoor",
        "_carpet",
        "_pane",
    ];

    let path = strip_namespace(name);

    for suffix in SUFFIXES {
        if let Some(base) = path.strip_suffix(suffix) {
            let base_name = format!("minecraft:{base}");
            if let Some(entry) = catalog.entries.get(&base_name) {
                return Some((entry.color, layer_for_name(name)));
            }
            if let Some(color) = family_color(&base_name, catalog) {
                return Some((color, layer_for_name(name)));
            }
        }
    }

    family_color(name, catalog).map(|color| (color, layer_for_name(name)))
}

fn family_color(name: &str, catalog: &ColorBlockCatalog) -> Option<Rgba> {
    let path = strip_namespace(name);

    if let Some(rest) = path.strip_prefix("stripped_") {
        return family_color(&format!("minecraft:{rest}"), catalog);
    }

    if path.ends_with("rail") {
        return Some(Rgba::rgb(138, 138, 138));
    }

    if path.contains("torch") {
        return Some(Rgba::rgb(240, 192, 96));
    }

    if let Some(base) = path.strip_suffix("_brick")
        && let Some(entry) = catalog.entries.get(&format!("minecraft:{base}_bricks"))
    {
        return Some(entry.color);
    }

    if path.contains("kelp") {
        return Some(Rgba::rgb(58, 122, 58));
    }

    if path.contains("skull") || path.ends_with("_head") {
        return Some(Rgba::rgb(200, 192, 176));
    }

    for (prefix, color) in DYE_COLORS {
        if let Some(suffix) = path.strip_prefix(prefix)
            && let Some(suffix) = suffix.strip_prefix('_')
            && matches!(
                suffix,
                "wool"
                    | "concrete"
                    | "concrete_powder"
                    | "terracotta"
                    | "stained_glass"
                    | "stained_glass_pane"
                    | "carpet"
                    | "bed"
                    | "banner"
                    | "wall_banner"
                    | "candle"
                    | "shulker_box"
                    | "glazed_terracotta"
            )
        {
            return Some(*color);
        }
    }

    for (wood, planks, bark, leaves) in WOOD_COLORS {
        if path == *wood || path.starts_with(&format!("{wood}_")) {
            if path.contains("leaves") {
                return Some(*leaves);
            }
            if path.contains("log")
                || path.contains("wood")
                || path.contains("stem")
                || path.contains("hyphae")
            {
                return Some(*bark);
            }
            return Some(*planks);
        }
    }

    if path.ends_with("_ore") || path.ends_with("_ore_deepslate") {
        return Some(ore_color(path));
    }

    if let Some(color) = plant_color(path) {
        return Some(color);
    }

    if path.contains("glass") {
        return Some(Rgba::rgb(160, 208, 224));
    }

    if path == "water" {
        return Some(Rgba::new(58, 111, 208, 180));
    }
    if path == "lava" {
        return Some(Rgba::rgb(208, 90, 26));
    }
    if path == "ice" || path == "packed_ice" || path == "blue_ice" {
        return Some(Rgba::new(160, 208, 240, 180));
    }

    if let Some(entry) = catalog.entries.get(name) {
        return Some(entry.color);
    }

    None
}

fn layer_for_name(name: &str) -> RenderLayer {
    let path = strip_namespace(name);

    if path.contains("glass_pane")
        || path.contains("stained_glass_pane")
        || path.contains("iron_bars")
        || path == "chain"
        || path.contains("leaves")
        || path.contains("sapling")
        || path.contains("vine")
        || path.contains("fern")
        || path.contains("flower")
        || path.contains("tulip")
        || path.contains("orchid")
        || path.contains("blossom")
        || path.contains("poppy")
        || path.contains("dandelion")
        || path.contains("allium")
        || path.contains("azure_bluet")
        || path.contains("oxeye_daisy")
        || path.contains("cornflower")
        || path.contains("sunflower")
        || path.contains("lilac")
        || path.contains("peony")
        || path.contains("rose")
        || path.contains("torch")
        || path.contains("rail")
        || path.contains("ladder")
        || path.contains("cobweb")
        || path.contains("kelp")
        || path == "short_grass"
        || path == "tall_grass"
        || path == "seagrass"
        || path == "lily_pad"
        || path == "dead_bush"
        || path.contains("bush")
        || path.contains("roots")
        || path.contains("sprouts")
        || path.contains("crop")
        || path.contains("wheat")
        || path.contains("carrots")
        || path.contains("potatoes")
        || path.contains("beetroots")
    {
        return RenderLayer::Cutout;
    }

    if path == "water"
        || path == "ice"
        || path == "packed_ice"
        || path == "blue_ice"
        || path.contains("glass")
        || path == "slime_block"
        || path == "honey_block"
    {
        return RenderLayer::Translucent;
    }

    RenderLayer::Opaque
}

fn strip_namespace(name: &str) -> &str {
    name.split_once(':').map(|(_, path)| path).unwrap_or(name)
}

fn plant_color(path: &str) -> Option<Rgba> {
    const GREEN: Rgba = Rgba::rgb(86, 140, 58);
    const DARK_GREEN: Rgba = Rgba::rgb(58, 110, 42);
    const PINK: Rgba = Rgba::rgb(224, 128, 168);
    const YELLOW: Rgba = Rgba::rgb(232, 200, 64);
    const BLUE: Rgba = Rgba::rgb(82, 112, 196);
    const WHITE: Rgba = Rgba::rgb(232, 236, 220);
    const RED: Rgba = Rgba::rgb(190, 56, 56);

    if path.contains("tulip") || path.contains("poppy") || path.contains("rose") {
        return Some(RED);
    }
    if path.contains("dandelion") || path.contains("sunflower") {
        return Some(YELLOW);
    }
    if path.contains("orchid") || path.contains("cornflower") {
        return Some(BLUE);
    }
    if path.contains("lily") || path.contains("azure_bluet") {
        return Some(WHITE);
    }
    if path.contains("cherry") || path.contains("pink") || path.contains("blossom") {
        return Some(PINK);
    }
    if path.contains("sapling")
        || path.contains("fern")
        || path.contains("vine")
        || path.contains("grass")
        || path.contains("bush")
        || path.contains("roots")
        || path.contains("sprouts")
        || path.contains("crop")
        || path.contains("wheat")
    {
        return Some(GREEN);
    }
    if path.contains("cactus") || path.contains("bamboo") || path.contains("sugar_cane") {
        return Some(DARK_GREEN);
    }

    None
}

fn ore_color(path: &str) -> Rgba {
    if path.contains("coal") {
        Rgba::rgb(50, 50, 50)
    } else if path.contains("iron") {
        Rgba::rgb(200, 160, 128)
    } else if path.contains("copper") {
        Rgba::rgb(200, 122, 74)
    } else if path.contains("gold") {
        Rgba::rgb(240, 208, 64)
    } else if path.contains("redstone") {
        Rgba::rgb(192, 32, 32)
    } else if path.contains("lapis") {
        Rgba::rgb(32, 64, 160)
    } else if path.contains("diamond") {
        Rgba::rgb(64, 224, 224)
    } else if path.contains("emerald") {
        Rgba::rgb(32, 192, 96)
    } else if path.contains("quartz") {
        Rgba::rgb(224, 224, 208)
    } else {
        Rgba::rgb(127, 127, 127)
    }
}

const DYE_COLORS: &[(&str, Rgba)] = &[
    ("light_blue", Rgba::rgb(58, 175, 217)),
    ("light_gray", Rgba::rgb(142, 142, 134)),
    ("magenta", Rgba::rgb(189, 68, 179)),
    ("orange", Rgba::rgb(240, 118, 19)),
    ("yellow", Rgba::rgb(248, 198, 39)),
    ("purple", Rgba::rgb(121, 42, 172)),
    ("black", Rgba::rgb(20, 21, 25)),
    ("brown", Rgba::rgb(114, 71, 40)),
    ("green", Rgba::rgb(84, 109, 27)),
    ("white", Rgba::rgb(233, 236, 236)),
    ("blue", Rgba::rgb(53, 57, 157)),
    ("cyan", Rgba::rgb(21, 137, 145)),
    ("gray", Rgba::rgb(62, 68, 71)),
    ("lime", Rgba::rgb(112, 185, 25)),
    ("pink", Rgba::rgb(237, 141, 172)),
    ("red", Rgba::rgb(161, 39, 34)),
];

const WOOD_COLORS: &[(&str, Rgba, Rgba, Rgba)] = &[
    (
        "oak",
        Rgba::rgb(184, 148, 95),
        Rgba::rgb(110, 84, 52),
        Rgba::rgb(79, 127, 42),
    ),
    (
        "spruce",
        Rgba::rgb(107, 79, 47),
        Rgba::rgb(58, 42, 26),
        Rgba::rgb(61, 95, 42),
    ),
    (
        "birch",
        Rgba::rgb(215, 193, 133),
        Rgba::rgb(216, 214, 200),
        Rgba::rgb(111, 143, 58),
    ),
    (
        "jungle",
        Rgba::rgb(165, 116, 74),
        Rgba::rgb(84, 60, 38),
        Rgba::rgb(63, 127, 42),
    ),
    (
        "acacia",
        Rgba::rgb(176, 93, 47),
        Rgba::rgb(104, 58, 34),
        Rgba::rgb(111, 143, 58),
    ),
    (
        "dark_oak",
        Rgba::rgb(74, 48, 24),
        Rgba::rgb(42, 28, 16),
        Rgba::rgb(58, 95, 42),
    ),
    (
        "mangrove",
        Rgba::rgb(122, 59, 46),
        Rgba::rgb(78, 42, 34),
        Rgba::rgb(79, 127, 58),
    ),
    (
        "cherry",
        Rgba::rgb(227, 183, 176),
        Rgba::rgb(110, 60, 58),
        Rgba::rgb(227, 160, 176),
    ),
    (
        "bamboo",
        Rgba::rgb(214, 195, 122),
        Rgba::rgb(122, 142, 62),
        Rgba::rgb(94, 154, 58),
    ),
    (
        "crimson",
        Rgba::rgb(122, 59, 74),
        Rgba::rgb(92, 32, 50),
        Rgba::rgb(122, 59, 74),
    ),
    (
        "warped",
        Rgba::rgb(59, 107, 106),
        Rgba::rgb(42, 74, 74),
        Rgba::rgb(59, 107, 106),
    ),
];
