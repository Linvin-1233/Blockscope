mod support;

use litematic_mesh::{MeshErrorCode, MeshLimits, SceneBlockView};
use support::{region as make_region, scene as make_scene};

#[test]
fn resolves_single_region_and_missing_positions() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[0, 1],
    );
    let scene = make_scene(vec![region]);
    let view = SceneBlockView::new(&scene, &MeshLimits::default()).unwrap();

    assert_eq!(
        view.resolve([0, 0, 0]).unwrap().state.canonical(),
        "minecraft:air"
    );
    assert_eq!(
        view.resolve([1, 0, 0]).unwrap().state.canonical(),
        "minecraft:stone"
    );
    assert!(view.resolve([2, 0, 0]).is_none());
    assert!(view.resolve([0, 0, 1]).is_none());
}

#[test]
fn later_region_wins_on_overlap() {
    let first = make_region("a", [0, 0, 0], [1, 1, 1], &["minecraft:stone"], &[0]);
    let second = make_region("b", [0, 0, 0], [1, 1, 1], &["minecraft:dirt"], &[0]);
    let scene = make_scene(vec![first, second]);
    let view = SceneBlockView::new(&scene, &MeshLimits::default()).unwrap();

    let resolved = view.resolve([0, 0, 0]).unwrap();
    assert_eq!(resolved.region_index, 1);
    assert_eq!(resolved.state.canonical(), "minecraft:dirt");
}

#[test]
fn handles_negative_coordinates_and_page_boundaries() {
    let region = make_region(
        "negative",
        [-1, 0, 0],
        [1, 1, 1],
        &["minecraft:stone"],
        &[0],
    );
    let scene = make_scene(vec![region]);
    let view = SceneBlockView::new(&scene, &MeshLimits::default()).unwrap();

    assert_eq!(view.page_key([-1, 0, 0]), Some([-1, 0, 0]));
    assert_eq!(view.page_key([0, 0, 0]), Some([0, 0, 0]));
    assert_eq!(
        view.resolve([-1, 0, 0]).unwrap().state.canonical(),
        "minecraft:stone"
    );
}

#[test]
fn air_only_pages_resolve_but_are_not_meshed() {
    let region = make_region("air", [0, 0, 0], [2, 1, 1], &["minecraft:air"], &[0, 0]);
    let scene = make_scene(vec![region]);
    let view = SceneBlockView::new(&scene, &MeshLimits::default()).unwrap();

    assert_eq!(view.pages(), 0);
    assert!(view.resolve([0, 0, 0]).unwrap().state.is_air());
    assert!(view.resolve([1, 0, 0]).unwrap().state.is_air());
}

#[test]
fn counts_pages_and_enforces_page_limit() {
    let values = vec![1u32; 17];
    let region = make_region(
        "wide",
        [0, 0, 0],
        [17, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    let scene = make_scene(vec![region]);
    let view = SceneBlockView::new(&scene, &MeshLimits::default()).unwrap();
    assert_eq!(view.pages(), 2);

    let values = vec![1u32; 33];
    let region = make_region(
        "wider",
        [0, 0, 0],
        [33, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &values,
    );
    let scene = make_scene(vec![region]);
    let error = SceneBlockView::new(&scene, &MeshLimits::small()).unwrap_err();
    assert_eq!(error.code, MeshErrorCode::PageLimitExceeded);
}
