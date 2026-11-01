// Repository automation. Run through the `cargo xtask` alias.

mod args;
mod error;
mod msi;
mod process;
mod registry;
mod tasks;
mod worker;

use std::env;

use args::Args;
use error::{Error, Result};

const USAGE: &str = "\
usage: cargo xtask <command> [options]

commands:
  check                     format check, clippy, and workspace tests
  check-linux               clippy against the Linux triple
  build-static              release CLI build with the static C runtime
  golden-update             regenerate golden manifests and PNGs
  test-real <kind>          ignored tests against local fixtures
                            kind: litematic | jar
                            --path <file>  fixture directory or client jar
                            -- <args>      forwarded to cargo test
  worker                    build the Cloudflare Worker and pack its output
  pack-worker               repack an existing worker build
  docker-smoke [--tag <t>]  capacity and shutdown check inside the image
  verify-tag <tag>          check a release tag against Cargo.toml
  build-msi                 build the per-user MSI (Windows only)
                            --output <path> [--bin-dir <dir>] [--version <v>]
  test-msi                  install/uninstall lifecycle check (Windows only)
                            [--msi <path>] [--log-dir <dir>]
  release-msi               build, checksum, and upload the MSI (Windows only)
                            --tag <tag> [--triple <triple>]";

fn main() {
    let Err(error) = run() else { return };
    if let Error::Message(message) = &error {
        eprintln!("{message}");
    }
    std::process::exit(error.exit_code());
}

fn run() -> Result<()> {
    let mut argv: Vec<String> = env::args().skip(1).collect();
    if argv.is_empty() {
        return Err(Error::message(USAGE));
    }

    let command = argv.remove(0);
    let args = Args::parse(argv)?;

    match command.as_str() {
        "check" => tasks::check(),
        "check-linux" => tasks::check_linux(),
        "build-static" => tasks::build_static(),
        "golden-update" => tasks::golden_update(),
        "test-real" => {
            let kind = args.require_positional(
                0,
                "usage: cargo xtask test-real <litematic|jar> [--path <file>] [-- <args>]",
            )?;
            tasks::test_real(kind, args.option("path"), &args.rest)
        }
        "docker-smoke" => tasks::docker_smoke(args.option("tag").unwrap_or("blockscope:local")),
        "verify-tag" => {
            let tag = args
                .positional(0)
                .or_else(|| args.option("tag"))
                .ok_or_else(|| Error::message("usage: cargo xtask verify-tag <tag>"))?;
            tasks::verify_tag(tag)
        }
        "worker" => worker::build(),
        "pack-worker" => worker::pack(),
        "build-msi" => msi::build(
            args.require_option("output")?,
            args.option("bin-dir").unwrap_or("target/release"),
            args.option("version"),
        ),
        "test-msi" => msi::test(args.option("msi"), args.option("log-dir")),
        "release-msi" => msi::release(
            args.require_option("tag")?,
            args.option("triple").unwrap_or("x86_64-pc-windows-msvc"),
        ),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            Ok(())
        }
        other => Err(Error::message(format!(
            "unknown command: {other}\n\n{USAGE}"
        ))),
    }
}
