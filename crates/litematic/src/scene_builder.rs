use crate::container::LitematicFile;
use crate::error::{LitematicError, LitematicErrorCode};
use litematic_scene::{
    Diagnostic, DiagnosticCode, DiagnosticSeverity, Region, Scene, SceneError, SceneErrorCode,
    SceneMetadata,
};

impl LitematicFile {
    /// Consume this container and build the canonical render scene.
    pub fn into_scene(self) -> Result<Scene, LitematicError> {
        let Self {
            version,
            sub_version,
            minecraft_data_version,
            metadata,
            regions,
        } = self;

        let scene_metadata = SceneMetadata {
            name: metadata.name,
            author: metadata.author,
            description: metadata.description,
            litematic_version: version,
            sub_version,
            minecraft_data_version,
            declared_region_count: metadata.declared_region_count,
            declared_total_volume: metadata.declared_total_volume,
            declared_total_blocks: metadata.declared_total_blocks,
            declared_block_entity_count: metadata.declared_block_entity_count,
            declared_block_tick_count: metadata.declared_block_tick_count,
            declared_mc_data_version: metadata.declared_mc_data_version,
            declared_mc_version: metadata.declared_mc_version,
            enclosing_size: metadata.enclosing_size,
            original_origin: metadata.original_origin,
            time_created: metadata.time_created,
            time_modified: metadata.time_modified,
            preview_image_pixels: metadata.preview_image_pixels,
        };

        let mut scene_regions = Vec::with_capacity(regions.len());
        let mut diagnostics = Vec::new();

        for raw in regions {
            let name = raw.name.clone();
            let origin = raw.origin;
            let size = raw.size_abs;
            let data_version = raw.data_version;
            let data = raw.into_data()?;
            diagnostics.extend(data.diagnostics);

            scene_regions.push(Region {
                name,
                origin,
                size,
                data_version,
                palette: data.palette,
                states: data.states,
                block_entities: data.block_entities,
                entity_count: data.entity_count,
                pending_block_tick_count: data.pending_block_tick_count,
                pending_fluid_tick_count: data.pending_fluid_tick_count,
            });
        }

        let mut scene =
            Scene::new(scene_metadata, scene_regions, diagnostics).map_err(map_scene_error)?;

        let metadata_diagnostics = metadata_diagnostics(&scene);
        scene.diagnostics.extend(metadata_diagnostics);

        Ok(scene)
    }
}

fn map_scene_error(error: SceneError) -> LitematicError {
    let code = match error.code {
        SceneErrorCode::EmptyScene | SceneErrorCode::RegionLayoutMismatch => {
            LitematicErrorCode::InvalidLitematic
        }
        SceneErrorCode::BoundsOverflow | SceneErrorCode::StatisticsOverflow => {
            LitematicErrorCode::ResourceLimit
        }
    };

    LitematicError::new(code, error.message)
}

fn metadata_diagnostics(scene: &Scene) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    compare_count(
        "RegionCount",
        scene.metadata.declared_region_count,
        scene.regions.len() as u64,
        &mut diagnostics,
    );
    compare_count(
        "TotalVolume",
        scene.metadata.declared_total_volume,
        scene.statistics.volume,
        &mut diagnostics,
    );
    compare_count(
        "TotalBlocks",
        scene.metadata.declared_total_blocks,
        scene.statistics.non_air,
        &mut diagnostics,
    );
    compare_count(
        "BlockEntityCount",
        scene.metadata.declared_block_entity_count,
        scene.statistics.block_entities,
        &mut diagnostics,
    );
    compare_count(
        "BlockTickCount",
        scene.metadata.declared_block_tick_count,
        scene.statistics.pending_block_ticks,
        &mut diagnostics,
    );

    if let Some(enclosing) = scene.metadata.enclosing_size {
        let declared = [
            u64::from(enclosing[0].unsigned_abs()),
            u64::from(enclosing[1].unsigned_abs()),
            u64::from(enclosing[2].unsigned_abs()),
        ];
        let actual = scene.bounds.size();

        if declared != actual {
            diagnostics.push(Diagnostic::new(
                DiagnosticCode::MetadataMismatch,
                DiagnosticSeverity::Warning,
                format!(
                    "declared EnclosingSize {declared:?} does not match scene bounds {actual:?}"
                ),
            ));
        }
    }

    if scene.metadata.minecraft_data_version >= 0 && scene.metadata.minecraft_data_version < 1631 {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::UnsupportedDataVersion,
            DiagnosticSeverity::Warning,
            format!(
                "Minecraft data version {} predates 1.13.2; block names are preserved without datafixer renames",
                scene.metadata.minecraft_data_version
            ),
        ));
    }

    diagnostics
}

fn compare_count(
    name: &str,
    declared: Option<i64>,
    actual: u64,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(declared) = declared else {
        return;
    };
    if declared < 0 {
        return;
    }

    let declared = u64::try_from(declared).unwrap_or(u64::MAX);
    if declared != actual {
        diagnostics.push(Diagnostic::new(
            DiagnosticCode::MetadataMismatch,
            DiagnosticSeverity::Warning,
            format!("declared {name} {declared} does not match computed {actual}"),
        ));
    }
}
