use litematic_scene::{
    BlockStateKey, Manifest, PackedIndices, Region, Scene, SceneMetadata, sha256,
};

fn metadata() -> SceneMetadata {
    SceneMetadata {
        name: Some("demo".to_string()),
        author: Some("tester".to_string()),
        litematic_version: 7,
        minecraft_data_version: 3700,
        ..Default::default()
    }
}

fn scene() -> Scene {
    let region = Region {
        name: "main".to_string(),
        origin: [0, 0, 0],
        size: [2, 1, 1],
        data_version: 3700,
        palette: vec![
            BlockStateKey::simple("minecraft:air"),
            BlockStateKey::simple("minecraft:stone"),
        ],
        states: PackedIndices::new(2, vec![4], 2).unwrap(),
        block_entities: Vec::new(),
        entity_count: 0,
        pending_block_tick_count: 0,
        pending_fluid_tick_count: 0,
    };

    Scene::new(metadata(), vec![region], Vec::new()).unwrap()
}

#[test]
fn manifest_json_is_deterministic_and_round_trips() {
    let scene = scene();
    let manifest = Manifest::from_scene(&scene, sha256(b"input"), "0.1.0");

    let first = manifest.to_json().unwrap();
    let second = manifest.to_json().unwrap();
    assert_eq!(first, second);

    assert!(first.contains("\"schemaVersion\":1"));
    assert!(first.contains("\"inputHash\":\""));
    assert!(first.contains("\"sceneHash\":\""));
    assert!(first.contains("\"nonAir\":1"));
    assert!(first.contains("\"minecraft:stone\":1"));

    let parsed: Manifest = serde_json::from_str(&first).unwrap();
    assert_eq!(parsed, manifest);

    let pretty = manifest.to_json_pretty().unwrap();
    let parsed_pretty: Manifest = serde_json::from_str(&pretty).unwrap();
    assert_eq!(parsed_pretty, manifest);
}

#[test]
fn manifest_reports_bounds_and_regions() {
    let manifest = Manifest::from_scene(&scene(), sha256(b"input"), "0.1.0");

    assert_eq!(manifest.bounds.min, [0, 0, 0]);
    assert_eq!(manifest.bounds.max_exclusive, [2, 1, 1]);
    assert_eq!(manifest.bounds.size, [2, 1, 1]);
    assert_eq!(manifest.regions.len(), 1);
    assert_eq!(manifest.regions[0].name, "main");
    assert_eq!(manifest.regions[0].volume, 2);
    assert_eq!(manifest.statistics.volume, 2);
    assert_eq!(manifest.statistics.air, 1);
    assert_eq!(manifest.statistics.non_air, 1);
}
