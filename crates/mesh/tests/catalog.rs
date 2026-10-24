use litematic_mesh::{BlockCatalog, CatalogErrorCode, ColorBlockCatalog, RenderLayer, Rgba};
use litematic_scene::BlockStateKey;

fn resolve(catalog: &ColorBlockCatalog, name: &str) -> litematic_mesh::ResolvedBlock {
    catalog.resolve(&BlockStateKey::simple(name))
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
