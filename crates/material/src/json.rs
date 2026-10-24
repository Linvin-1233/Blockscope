use serde::Deserialize;
use std::collections::BTreeMap;

/// A parsed `blockstates/<name>.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BlockstateFile {
    #[serde(default)]
    pub variants: BTreeMap<String, VariantValue>,
    #[serde(default)]
    pub multipart: Vec<Multipart>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VariantValue {
    One(Variant),
    Many(Vec<Variant>),
}

impl VariantValue {
    pub fn first(&self) -> Option<&Variant> {
        match self {
            Self::One(variant) => Some(variant),
            Self::Many(variants) => variants.first(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Variant {
    pub model: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub uvlock: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Multipart {
    pub apply: VariantValue,
    #[serde(default)]
    pub when: Option<Condition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Condition {
    And(AndCondition),
    Or(OrCondition),
    Props(BTreeMap<String, String>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct AndCondition {
    #[serde(rename = "AND")]
    pub and: Vec<Condition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrCondition {
    #[serde(rename = "OR")]
    pub or: Vec<Condition>,
}

/// A parsed `models/<path>.json`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelFile {
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub textures: BTreeMap<String, TextureRef>,
    #[serde(default)]
    pub elements: Option<Vec<Element>>,
    #[serde(default)]
    pub ambientocclusion: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum TextureRef {
    Path(String),
    Sprite(SpriteRef),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpriteRef {
    pub sprite: String,
    #[serde(default)]
    pub force_translucent: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Element {
    pub from: [f32; 3],
    pub to: [f32; 3],
    #[serde(default)]
    pub rotation: Option<ElementRotation>,
    #[serde(default)]
    pub shade: Option<bool>,
    #[serde(default)]
    pub faces: BTreeMap<String, Face>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ElementRotation {
    pub origin: [f32; 3],
    pub axis: String,
    pub angle: f32,
    #[serde(default)]
    pub rescale: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Face {
    #[serde(default)]
    pub uv: Option<[f32; 4]>,
    pub texture: String,
    #[serde(default)]
    pub cullface: Option<String>,
    #[serde(default)]
    pub rotation: Option<i32>,
    #[serde(default)]
    pub tintindex: Option<i32>,
}
