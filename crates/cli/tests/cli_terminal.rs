mod support;

use support::*;
#[test]
fn render_terminal_kitty_outputs_apc() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["render", path, "--terminal-protocol", "kitty", "--no-file"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1b_G"));
    assert!(stdout.contains("\x1b\\"));
    assert!(stdout.contains("output: terminal (kitty)"));
}

#[test]
fn render_terminal_iterm2_outputs_osc() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["render", path, "--terminal-protocol", "iterm2", "--no-file"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1b]1337;File="));
    assert!(stdout.contains("inline=1"));
    assert!(stdout.contains("output: terminal (iterm2)"));
}

#[test]
fn render_terminal_sixel_outputs_dcs() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["render", path, "--terminal-protocol", "sixel", "--no-file"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("\x1bPq"));
    assert!(stdout.contains("\x1b\\"));
    assert!(stdout.contains("output: terminal (sixel)"));
}

#[test]
fn render_terminal_halfblocks_outputs_unicode() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&[
        "render",
        path,
        "--terminal-protocol",
        "halfblocks",
        "--terminal-width",
        "40",
        "--terminal-height",
        "20",
        "--no-file",
    ]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains('▀') || stdout.contains('▄'));
    assert!(stdout.contains("\x1b[38;2;"));
    assert!(stdout.contains("output: terminal (halfblocks)"));
}

#[test]
fn render_terminal_no_file_does_not_create_output_file() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("should-not-exist.png");

    let output = run(&[
        "render",
        path,
        "--output",
        output_path.to_str().unwrap(),
        "-t",
        "--no-file",
    ]);
    assert!(output.status.success());
    assert!(!output_path.exists());
}

#[test]
fn preview_and_term_aliases_work() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let preview = run(&["preview", path, "-t", "--no-file"]);
    assert!(preview.status.success());

    let term = run(&["term", path, "-t", "--no-file"]);
    assert!(term.status.success());
}

#[test]
fn render_rejects_invalid_terminal_protocol() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&[
        "render",
        path,
        "--terminal-protocol",
        "nonexistent-protocol",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("INVALID_TERMINAL_PROTOCOL"));
}

#[test]
fn render_terminal_prints_summary_before_the_image() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["render", path, "--terminal-protocol", "kitty", "--no-file"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);

    let summary = stdout.find("output: terminal (kitty)").expect("summary");
    let image = stdout.find("\x1b_G").expect("kitty graphics");
    assert!(
        summary < image,
        "summary must precede the image so text cannot scroll it away"
    );
}

#[test]
fn render_terminal_warns_when_stdout_is_not_a_tty() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = run(&["render", path, "-t", "--no-file"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("stdout is not a terminal"), "{stderr}");
}

#[test]
fn render_terminal_falls_back_to_halfblocks_inside_tmux() {
    let file = write_fixture(&basic_fixture());
    let path = file.path().to_str().unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_blockscope"))
        .args(["render", path, "--terminal", "--no-file"])
        .env("TMUX", "/tmp/tmux-501/default,1,0")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("output: terminal (halfblocks)"), "{stdout}");
    assert!(!stdout.contains("\x1b_G"), "no kitty graphics inside tmux");
}
