use crate::Scene;
use crate::hash::Sha256Digest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of the JSON manifest schema.
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub schema_version: u32,
    pub parser: ParserInfo,
    pub input_hash: String,
    pub scene_hash: String,
    pub metadata: ManifestMetadata,
    pub bounds: ManifestBounds,
    pub regions: Vec<ManifestRegion>,
    pub statistics: ManifestStatistics,
    pub diagnostics: Vec<ManifestDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserInfo {
    pub version: String,
    pub litematic_version: i32,
    pub minecraft_data_version: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestMetadata {
    pub name: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
    pub litematic_version: i32,
    pub sub_version: Option<i32>,
    pub minecraft_data_version: i32,
    pub declared_region_count: Option<i64>,
    pub declared_total_volume: Option<i64>,
    pub declared_total_blocks: Option<i64>,
    pub declared_block_entity_count: Option<i64>,
    pub declared_block_tick_count: Option<i64>,
    pub declared_mc_data_version: Option<i64>,
    pub declared_mc_version: Option<String>,
    pub enclosing_size: Option<[i32; 3]>,
    pub original_origin: Option<[i32; 3]>,
    pub time_created: Option<i64>,
    pub time_modified: Option<i64>,
    pub preview_image_pixels: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestBounds {
    pub min: [i64; 3],
    pub max_exclusive: [i64; 3],
    pub size: [u64; 3],
    pub volume: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRegion {
    pub name: String,
    pub origin: [i64; 3],
    pub size: [u32; 3],
    pub data_version: i32,
    pub volume: u64,
    pub block_entities: u64,
    pub entities: u64,
    pub pending_block_ticks: u64,
    pub pending_fluid_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestStatistics {
    pub volume: u64,
    pub air: u64,
    pub non_air: u64,
    pub block_entities: u64,
    pub entities: u64,
    pub pending_block_ticks: u64,
    pub pending_fluid_ticks: u64,
    pub blocks: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestDiagnostic {
    pub code: String,
    pub severity: String,
    pub message: String,
    pub region: Option<String>,
}

impl Manifest {
    pub fn from_scene(
        scene: &Scene,
        input_hash: Sha256Digest,
        parser_version: impl Into<String>,
    ) -> Self {
        let metadata = &scene.metadata;
        let statistics = &scene.statistics;

        let regions = scene
            .regions
            .iter()
            .map(|region| ManifestRegion {
                name: region.name.clone(),
                origin: region.origin,
                size: region.size,
                data_version: region.data_version,
                volume: region.states.volume(),
                block_entities: region.block_entities.len() as u64,
                entities: region.entity_count as u64,
                pending_block_ticks: region.pending_block_tick_count as u64,
                pending_fluid_ticks: region.pending_fluid_tick_count as u64,
            })
            .collect();

        let blocks = statistics
            .blocks
            .iter()
            .map(|(key, count)| (key.canonical(), *count))
            .collect::<BTreeMap<_, _>>();

        let diagnostics = scene
            .diagnostics
            .iter()
            .map(|diagnostic| ManifestDiagnostic {
                code: diagnostic.code.as_str().to_string(),
                severity: diagnostic.severity.as_str().to_string(),
                message: diagnostic.message.clone(),
                region: diagnostic.region.clone(),
            })
            .collect();

        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            parser: ParserInfo {
                version: parser_version.into(),
                litematic_version: metadata.litematic_version,
                minecraft_data_version: metadata.minecraft_data_version,
            },
            input_hash: input_hash.to_hex(),
            scene_hash: scene.scene_hash().to_hex(),
            metadata: ManifestMetadata {
                name: metadata.name.clone(),
                author: metadata.author.clone(),
                description: metadata.description.clone(),
                litematic_version: metadata.litematic_version,
                sub_version: metadata.sub_version,
                minecraft_data_version: metadata.minecraft_data_version,
                declared_region_count: metadata.declared_region_count,
                declared_total_volume: metadata.declared_total_volume,
                declared_total_blocks: metadata.declared_total_blocks,
                declared_block_entity_count: metadata.declared_block_entity_count,
                declared_block_tick_count: metadata.declared_block_tick_count,
                declared_mc_data_version: metadata.declared_mc_data_version,
                declared_mc_version: metadata.declared_mc_version.clone(),
                enclosing_size: metadata.enclosing_size,
                original_origin: metadata.original_origin,
                time_created: metadata.time_created,
                time_modified: metadata.time_modified,
                preview_image_pixels: metadata.preview_image_pixels,
            },
            bounds: ManifestBounds {
                min: scene.bounds.min,
                max_exclusive: scene.bounds.max_exclusive,
                size: scene.bounds.size(),
                volume: scene.bounds.volume(),
            },
            regions,
            statistics: ManifestStatistics {
                volume: statistics.volume,
                air: statistics.air,
                non_air: statistics.non_air,
                block_entities: statistics.block_entities,
                entities: statistics.entities,
                pending_block_ticks: statistics.pending_block_ticks,
                pending_fluid_ticks: statistics.pending_fluid_ticks,
                blocks,
            },
            diagnostics,
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
