use litematic_mesh::{BlockCatalog, CatalogErrorCode, ColorBlockCatalog, RenderLayer, Rgba};
use litematic_scene::BlockStateKey;
use std::collections::BTreeMap;

fn resolve(catalog: &ColorBlockCatalog, name: &str) -> litematic_mesh::ResolvedBlock {
    catalog.resolve(&BlockStateKey::simple(name))
}

fn state(name: &str, properties: &[(&str, &str)]) -> BlockStateKey {
    let properties = properties
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect::<BTreeMap<_, _>>();
    BlockStateKey::new(name, properties)
}

#[test]
fn resolves_exact_and_family_colors() {
    let catalog = ColorBlockCatalog::default();

    let stone = resolve(&catalog, "minecraft:stone");
    assert!(stone.known);
    assert_eq!(stone.layer, RenderLayer::Opaque);
    assert!(stone.model.is_full_cube);
    assert_eq!(stone.color, Rgba::rgb(127, 127, 127));

    let wool = resolve(&catalog, "minecraft:red_wool");
    assert!(wool.known);
    assert_eq!(wool.color, Rgba::rgb(161, 39, 34));

    let planks = resolve(&catalog, "minecraft:oak_planks");
    assert!(planks.known);
    assert_eq!(planks.color, Rgba::rgb(184, 148, 95));

    let log = resolve(&catalog, "minecraft:oak_log");
    assert!(log.known);
    assert_eq!(log.color, Rgba::rgb(110, 84, 52));

    let leaves = resolve(&catalog, "minecraft:oak_leaves");
    assert!(leaves.known);
    assert_eq!(leaves.layer, RenderLayer::Cutout);

    let stairs = resolve(&catalog, "minecraft:stone_stairs");
    assert!(stairs.known);
    assert_eq!(stairs.color, Rgba::rgb(127, 127, 127));
}

#[test]
fn classifies_render_layers() {
    let catalog = ColorBlockCatalog::default();

    assert_eq!(
        resolve(&catalog, "minecraft:glass").layer,
        RenderLayer::Translucent
    );
    assert_eq!(
        resolve(&catalog, "minecraft:water").layer,
        RenderLayer::Translucent
    );
    assert_eq!(
        resolve(&catalog, "minecraft:oak_leaves").layer,
        RenderLayer::Cutout
    );
    assert_eq!(
        resolve(&catalog, "minecraft:grass_block").layer,
        RenderLayer::Opaque
    );
    assert_eq!(
        resolve(&catalog, "minecraft:white_stained_glass_pane").layer,
        RenderLayer::Cutout
    );
}

#[test]
fn unknown_blocks_use_magenta_fallback() {
    let catalog = ColorBlockCatalog::default();
    let unknown = resolve(&catalog, "modded:unknown_block");

    assert!(!unknown.known);
    assert_eq!(unknown.color, Rgba::MAGENTA);
    assert!(unknown.model.is_full_cube);
}

#[test]
fn air_is_not_a_solid_full_cube() {
    let catalog = ColorBlockCatalog::default();
    let air = resolve(&catalog, "minecraft:air");

    assert!(air.known);
    assert!(!air.model.is_full_cube);
    assert!(air.model.quads.is_empty());
    assert_eq!(air.color, Rgba::TRANSPARENT);
}

#[test]
fn custom_json_overrides_color_and_layer() {
    let catalog = ColorBlockCatalog::from_json_str(
        r##"{"blocks":{"minecraft:stone":{"color":"#112233","layer":"cutout"}}}"##,
    )
    .unwrap();
    let stone = resolve(&catalog, "minecraft:stone");

    assert_eq!(stone.color, Rgba::rgb(0x11, 0x22, 0x33));
    assert_eq!(stone.layer, RenderLayer::Cutout);
}

#[test]
fn resolves_partial_shapes() {
    let catalog = ColorBlockCatalog::default();

    let slab = resolve(&catalog, "minecraft:oak_slab");
    assert!(!slab.model.is_full_cube);
    assert_eq!(slab.model.quads.len(), 6);

    let double_slab = catalog.resolve(&state("minecraft:oak_slab", &[("type", "double")]));
    assert!(double_slab.model.is_full_cube);
    assert_eq!(double_slab.model.quads.len(), 6);

    let stairs = catalog.resolve(&state(
        "minecraft:oak_stairs",
        &[
            ("facing", "east"),
            ("half", "bottom"),
            ("shape", "straight"),
        ],
    ));
    assert!(!stairs.model.is_full_cube);
    assert_eq!(stairs.model.quads.len(), 10);

    let carpet = resolve(&catalog, "minecraft:white_carpet");
    assert!(!carpet.model.is_full_cube);
    assert_eq!(carpet.model.quads.len(), 6);

    let snow = catalog.resolve(&state("minecraft:snow", &[("layers", "4")]));
    assert!(!snow.model.is_full_cube);
    assert_eq!(snow.model.quads.len(), 6);

    let pane = resolve(&catalog, "minecraft:glass_pane");
    assert!(!pane.model.is_full_cube);

    let grass = resolve(&catalog, "minecraft:short_grass");
    assert!(!grass.model.is_full_cube);
    assert_eq!(grass.model.quads.len(), 4);

    let fence = catalog.resolve(&state("minecraft:oak_fence", &[("north", "true")]));
    assert!(!fence.model.is_full_cube);
    assert!(fence.model.quads.len() > 6);

    let wall = resolve(&catalog, "minecraft:stone_brick_wall");
    assert!(wall.known);
    assert!(!wall.model.is_full_cube);

    let door = resolve(&catalog, "minecraft:oak_door");
    assert!(!door.model.is_full_cube);
    assert_eq!(door.model.quads.len(), 6);

    let trapdoor = resolve(&catalog, "minecraft:oak_trapdoor");
    assert!(!trapdoor.model.is_full_cube);

    let button = resolve(&catalog, "minecraft:oak_button");
    assert!(!button.model.is_full_cube);

    let torch = resolve(&catalog, "minecraft:torch");
    assert!(!torch.model.is_full_cube);

    let rail = resolve(&catalog, "minecraft:rail");
    assert!(!rail.model.is_full_cube);

    let powered_rail = resolve(&catalog, "minecraft:powered_rail");
    assert!(powered_rail.known);
    assert!(!powered_rail.model.is_full_cube);

    let stripped = resolve(&catalog, "minecraft:stripped_oak_log");
    assert!(stripped.known);
    assert_eq!(stripped.color, Rgba::rgb(110, 84, 52));
}

#[test]
fn resolves_fluids_and_static_block_entities() {
    let catalog = ColorBlockCatalog::default();

    let source = catalog.resolve(&state("minecraft:water", &[("level", "0")]));
    assert!(source.model.is_full_cube);
    assert_eq!(source.layer, RenderLayer::Translucent);

    let flowing = catalog.resolve(&state("minecraft:water", &[("level", "8")]));
    assert!(!flowing.model.is_full_cube);
    assert_eq!(flowing.model.quads.len(), 6);

    let lava = catalog.resolve(&state("minecraft:lava", &[("level", "4")]));
    assert!(!lava.model.is_full_cube);
    assert_eq!(lava.layer, RenderLayer::Opaque);

    let chest = resolve(&catalog, "minecraft:chest");
    assert!(!chest.model.is_full_cube);
    assert_eq!(chest.model.quads.len(), 6);

    let ender_chest = resolve(&catalog, "minecraft:ender_chest");
    assert!(!ender_chest.model.is_full_cube);

    let sign = resolve(&catalog, "minecraft:oak_sign");
    assert!(!sign.model.is_full_cube);
    assert_eq!(sign.model.quads.len(), 6);

    let banner = resolve(&catalog, "minecraft:white_banner");
    assert!(banner.known);
    assert!(!banner.model.is_full_cube);

    let skull = resolve(&catalog, "minecraft:skeleton_skull");
    assert!(skull.known);
    assert!(!skull.model.is_full_cube);

    let lectern = resolve(&catalog, "minecraft:lectern");
    assert!(!lectern.model.is_full_cube);

    let beacon = resolve(&catalog, "minecraft:beacon");
    assert!(beacon.model.is_full_cube);
    assert_eq!(beacon.layer, RenderLayer::Translucent);

    let conduit = resolve(&catalog, "minecraft:conduit");
    assert!(!conduit.model.is_full_cube);
    assert_eq!(conduit.layer, RenderLayer::Translucent);

    let shulker = resolve(&catalog, "minecraft:white_shulker_box");
    assert!(shulker.known);
    assert!(!shulker.model.is_full_cube);
}

#[test]
fn invalid_palette_data_is_rejected() {
    let error = ColorBlockCatalog::from_json_str("not json").unwrap_err();
    assert_eq!(error.code, CatalogErrorCode::InvalidJson);

    let error =
        ColorBlockCatalog::from_json_str(r#"{"blocks":{"minecraft:stone":{"color":"nope"}}}"#)
            .unwrap_err();
    assert_eq!(error.code, CatalogErrorCode::InvalidColor);

    let error = ColorBlockCatalog::from_json_str(
        r##"{"blocks":{"minecraft:stone":{"color":"#112233","layer":"shiny"}}}"##,
    )
    .unwrap_err();
    assert_eq!(error.code, CatalogErrorCode::InvalidLayer);
}
