use std::env;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output};

use crate::error::{Error, Result};

// Repository root. Resolved from the crate location so every command works
// regardless of the caller's working directory.
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives directly under the repository root")
        .to_path_buf()
}

// Reuse the cargo that launched xtask so the child picks the same toolchain.
pub fn cargo() -> Command {
    Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

pub fn run(command: &mut Command) -> Result<()> {
    let status = run_status(command)?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Status(status))
    }
}

pub fn run_status(command: &mut Command) -> Result<ExitStatus> {
    command
        .status()
        .map_err(|error| spawn_error(command.get_program(), error))
}

pub fn output(command: &mut Command) -> Result<Output> {
    command
        .output()
        .map_err(|error| spawn_error(command.get_program(), error))
}

fn spawn_error(program: &OsStr, error: std::io::Error) -> Error {
    Error::message(format!("{}: {error}", program.to_string_lossy()))
}

pub fn has_command(name: &str) -> bool {
    let locator = if cfg!(windows) { "where" } else { "which" };
    Command::new(locator)
        .arg(name)
        .output()
        .is_ok_and(|output| output.status.success())
}

pub fn require_windows(command: &str) -> Result<()> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(Error::message(format!("{command} only runs on Windows")))
    }
}

pub fn workspace_version() -> Result<String> {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml"))?;
    manifest
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .map(str::to_owned)
        .ok_or_else(|| Error::message("Cannot read [workspace.package] version from Cargo.toml"))
}
