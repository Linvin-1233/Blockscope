mod support;

use support::*;

#[test]
fn no_args_prints_welcome_and_exits_zero() {
    let output = run(&[]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("Blockscope v{}", env!("CARGO_PKG_VERSION"))),
        "{stdout}"
    );
    assert!(stdout.contains("Quick start"), "{stdout}");
    assert!(stdout.contains("blockscope render"), "{stdout}");
    assert!(stdout.contains("blockscope help"), "{stdout}");
}

#[test]
fn welcome_is_plain_when_piped() {
    let output = run(&[]);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("\u{1b}["), "{stdout:?}");
}

#[test]
fn color_always_emits_ansi() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["--color", "always", "validate", path]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{1b}["), "{stdout:?}");
    assert!(stdout.contains("OK:"), "{stdout}");
}

#[test]
fn color_never_emits_no_ansi() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["--color", "never", "validate", path]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("\u{1b}["), "{stdout:?}");
}

#[test]
fn color_flag_is_global_and_works_after_the_subcommand() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["validate", path, "--color", "always"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("\u{1b}["), "{stdout:?}");
}

#[test]
fn json_is_never_styled() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["--color", "always", "inspect", path, "--json"]);

    assert!(output.status.success());
    assert!(!output.stdout.windows(2).any(|bytes| bytes == b"\x1b["));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schemaVersion"], 1);
}

#[test]
fn help_lists_render_options() {
    let output = run(&["render", "--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Options:"), "{stdout}");
    for option in [
        "--output",
        "--width",
        "--height",
        "--yaw",
        "--elevation",
        "--threads",
        "--stream-mesh",
        "--terminal-protocol",
        "--client-jar",
        "--texture-filter",
        "--force-limits",
        "--ignore-block-entity-nbt",
    ] {
        assert!(stdout.contains(option), "missing {option} in {stdout}");
    }
    assert!(stdout.contains("Examples:"), "{stdout}");
}

#[test]
fn help_mentions_color() {
    let output = run(&["--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--color <WHEN>"), "{stdout}");
    assert!(stdout.contains("NO_COLOR"), "{stdout}");
}

#[test]
fn render_piped_has_no_progress() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let output = run(&["render", path, "--no-file"]);

    assert!(output.status.success());
    assert!(
        output.stderr.is_empty(),
        "stderr must stay quiet when piped: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn error_prefix_is_plain_when_piped() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("missing.litematic");
    let output = run(&["inspect", path.to_str().unwrap()]);

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.starts_with("error:"), "{stderr:?}");
    assert!(!stderr.contains("\u{1b}["), "{stderr:?}");
    assert!(stderr.contains("IO_ERROR"), "{stderr}");
}
