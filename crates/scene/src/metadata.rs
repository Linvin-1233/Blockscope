/// Metadata carried from the litematic container into the scene manifest.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneMetadata {
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
