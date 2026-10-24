use litematic_core::{DecodeLimits, LitematicFile, LitematicLimits};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "requires LITEMATIC_FIXTURES_DIR"]
fn parses_real_litematic_fixtures() {
    let Some(directory) = std::env::var_os("LITEMATIC_FIXTURES_DIR") else {
        eprintln!("LITEMATIC_FIXTURES_DIR is not set; skipping real fixture validation");
        return;
    };

    let directory = PathBuf::from(directory);
    let mut paths: Vec<PathBuf> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new("litematic")))
        .collect();
    paths.sort();

    if paths.is_empty() {
        eprintln!(
            "no .litematic files found in {}; nothing to validate",
            directory.display()
        );
        return;
    }

    for path in paths {
        let bytes = fs::read(&path).unwrap();
        let file = LitematicFile::parse_bytes(
            &bytes,
            &DecodeLimits::default(),
            &LitematicLimits::default(),
        )
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let scene = file
            .into_scene()
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));

        eprintln!(
            "{}: version={} regions={} volume={} non-air={} diagnostics={}",
            path.display(),
            scene.metadata.litematic_version,
            scene.regions.len(),
            scene.statistics.volume,
            scene.statistics.non_air,
            scene.diagnostics.len()
        );
    }
}
