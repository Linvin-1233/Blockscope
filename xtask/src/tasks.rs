// Commands that wrap cargo and docker for local development and CI.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::process::{cargo, has_command, output, run, workspace_version};

const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";

pub fn check() -> Result<()> {
    run(cargo().args(["fmt", "--all", "--check"]))?;
    run(cargo().args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]))?;
    run(cargo().args(["test", "--workspace"]))
}

pub fn check_linux() -> Result<()> {
    if !has_command("rustup") {
        eprintln!("rustup not found; skipping the {LINUX_TARGET} clippy check");
        return Ok(());
    }

    let installed = output(Command::new("rustup").args(["target", "list", "--installed"]))?;
    let installed = String::from_utf8_lossy(&installed.stdout);
    if !installed.lines().any(|line| line.trim() == LINUX_TARGET) {
        run(Command::new("rustup").args(["target", "add", LINUX_TARGET]))?;
    }

    run(cargo().args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--target",
        LINUX_TARGET,
        "--",
        "-D",
        "warnings",
    ]))
}

pub fn build_static() -> Result<()> {
    run(cargo()
        .args([
            "build",
            "--release",
            "--locked",
            "-p",
            "blockscope-cli",
            "--bin",
            "blockscope",
            "--features",
            "gpu",
        ])
        .env("RUSTFLAGS", "-C target-feature=+crt-static"))
}

pub fn golden_update() -> Result<()> {
    run(cargo()
        .args(["test", "-p", "blockscope-cli", "--test", "golden"])
        .env("UPDATE_GOLDEN", "1"))?;
    run(cargo()
        .args(["test", "-p", "blockscope-render", "--test", "golden_png"])
        .env("UPDATE_GOLDEN", "1"))
}

pub fn test_real(kind: &str, path: Option<&str>, extra: &[String]) -> Result<()> {
    let (env_var, tests): (&str, &[(&str, &str)]) = match kind {
        "litematic" => (
            "LITEMATIC_FIXTURES_DIR",
            &[("blockscope-core", "real_files")],
        ),
        "jar" => (
            "MINECRAFT_CLIENT_JAR",
            &[
                ("blockscope-material", "real_jar"),
                ("blockscope-render", "real_materials"),
            ],
        ),
        other => return Err(Error::message(format!("Unknown real test type: {other}"))),
    };

    for (package, test) in tests {
        let mut command = cargo();
        command
            .args(["test", "-p", package, "--test", test, "--", "--ignored"])
            .args(extra);
        if let Some(path) = path.filter(|value| !value.is_empty()) {
            command.env(env_var, path);
        }
        run(&mut command)?;
    }

    Ok(())
}

pub fn docker_smoke(tag: &str) -> Result<()> {
    let limits = ["--cpus=1", "--memory=1g"];

    run(Command::new("docker")
        .args(["run", "--rm"])
        .args(limits)
        .args([tag, "capacity", "--json"]))?;

    // The daemon exits once stdin closes, so send the shutdown request and wait
    // for a clean exit.
    let mut child = Command::new("docker")
        .args(["run", "--rm", "-i"])
        .args(limits)
        .args([tag, "serve"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| Error::message(format!("docker: {error}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(b"{\"type\":\"shutdown\"}\n")?;
    }

    let status = child
        .wait()
        .map_err(|error| Error::message(format!("docker: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Status(status))
    }
}

pub fn verify_tag(tag: &str) -> Result<()> {
    let version = workspace_version()?;
    if tag.trim_start_matches('v') != version {
        return Err(Error::message(format!(
            "Tag {tag} does not match workspace version {version}"
        )));
    }

    println!("Tag {tag} matches workspace version {version}");
    Ok(())
}
