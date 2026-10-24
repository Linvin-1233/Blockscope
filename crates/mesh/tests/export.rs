mod support;

use litematic_mesh::{ColorBlockCatalog, MeshLimits, build_mesh, write_obj, write_ply};
use support::{region as make_region, scene as make_scene};

fn mesh(scene: &litematic_scene::Scene) -> litematic_mesh::Mesh {
    build_mesh(scene, &ColorBlockCatalog::default(), &MeshLimits::default()).unwrap()
}

#[test]
fn obj_contains_page_adjusted_vertices_and_faces() {
    let region = make_region(
        "main",
        [16, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    );
    let mesh = mesh(&make_scene(vec![region]));
    let mut output = Vec::new();
    write_obj(&mesh, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();

    assert!(text.starts_with("# litematic mesh\n"));
    assert_eq!(
        text.lines().filter(|line| line.starts_with("v ")).count(),
        24
    );
    assert_eq!(
        text.lines().filter(|line| line.starts_with("f ")).count(),
        6
    );
    assert!(
        text.lines()
            .any(|line| line.starts_with("v 16 ") || line.starts_with("v 16.0 "))
    );
}

#[test]
fn ply_contains_colored_vertices_and_quads() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [1, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1],
    );
    let mesh = mesh(&make_scene(vec![region]));
    let mut output = Vec::new();
    write_ply(&mesh, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();

    assert!(text.starts_with("ply\n"));
    assert!(text.contains("element vertex 24"));
    assert!(text.contains("element face 6"));
    assert!(text.contains("property uchar red"));
    assert!(text.contains("end_header"));
}

#[test]
fn exports_are_deterministic() {
    let region = make_region(
        "main",
        [0, 0, 0],
        [2, 1, 1],
        &["minecraft:air", "minecraft:stone"],
        &[1, 1],
    );
    let mesh = mesh(&make_scene(vec![region]));

    let mut first_obj = Vec::new();
    let mut second_obj = Vec::new();
    write_obj(&mesh, &mut first_obj).unwrap();
    write_obj(&mesh, &mut second_obj).unwrap();
    assert_eq!(first_obj, second_obj);

    let mut first_ply = Vec::new();
    let mut second_ply = Vec::new();
    write_ply(&mesh, &mut first_ply).unwrap();
    write_ply(&mesh, &mut second_ply).unwrap();
    assert_eq!(first_ply, second_ply);
}

#[test]
fn empty_mesh_exports_valid_headers() {
    let region = make_region("main", [0, 0, 0], [1, 1, 1], &["minecraft:air"], &[0]);
    let mesh = mesh(&make_scene(vec![region]));

    let mut obj = Vec::new();
    write_obj(&mesh, &mut obj).unwrap();
    assert_eq!(String::from_utf8(obj).unwrap(), "# litematic mesh\n");

    let mut ply = Vec::new();
    write_ply(&mesh, &mut ply).unwrap();
    let text = String::from_utf8(ply).unwrap();
    assert!(text.contains("element vertex 0"));
    assert!(text.contains("element face 0"));
    assert!(text.contains("end_header"));
}
