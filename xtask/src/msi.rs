// Windows installer tasks: build the WiX MSI, verify its lifecycle, and upload
// the release asset.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::process::{require_windows, root, run, run_status, workspace_version};
use crate::registry;

const UNINSTALL_ROOTS: [&str; 2] = [
    "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
];
const PRODUCTS_KEY: &str = "HKCU\\Software\\Microsoft\\Installer\\Products";

pub fn build(output: &str, bin_dir: &str, version: Option<&str>) -> Result<()> {
    require_windows("build-msi")?;

    let version = match version {
        Some(version) => version.to_owned(),
        None => workspace_version()?,
    };
    // Windows Installer versions are numeric; pre-release suffixes cannot be
    // encoded.
    if version.contains('-') {
        return Err(Error::message(format!(
            "MSI version cannot carry a pre-release suffix: {version}"
        )));
    }

    let toolchain = Toolchain::detect()?;
    if !toolchain.present("wix") {
        run(toolchain.command("dotnet").args([
            "tool",
            "install",
            "--global",
            "wix",
            "--version",
            "6.0.2",
        ]))?;
    }

    let binaries = root().join(bin_dir);
    if !binaries.is_dir() {
        return Err(Error::message(format!(
            "binary directory does not exist: {}",
            binaries.display()
        )));
    }

    let target = root().join(output);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut wix = toolchain.command("wix");
    wix.arg("build")
        .arg(root().join("installer/blockscope.wxs"))
        .arg("-arch")
        .arg("x64")
        .arg("-d")
        .arg(format!("Version={version}"))
        .arg("-d")
        .arg(format!("BinDir={}", binaries.display()))
        .arg("-o")
        .arg(&target);
    run(&mut wix)
}

pub fn test(msi: Option<&str>, log_dir: Option<&str>) -> Result<()> {
    require_windows("test-msi")?;

    let msi = root().join(msi.unwrap_or("out/blockscope-msi-check.msi"));
    if !msi.is_file() {
        return Err(Error::message(format!("MSI not found: {}", msi.display())));
    }

    let log_dir = root().join(log_dir.unwrap_or("out"));
    fs::create_dir_all(&log_dir)?;
    let install_log = log_dir.join("install.log");
    let uninstall_log = log_dir.join("uninstall.log");

    let local_app_data =
        env::var_os("LOCALAPPDATA").ok_or_else(|| Error::message("LOCALAPPDATA is not set"))?;
    let install_dir = PathBuf::from(local_app_data)
        .join("Programs")
        .join("Blockscope");
    let executable = install_dir.join("blockscope.exe");

    msiexec("MSI install", "/i", &msi, &install_log)?;

    if !executable.is_file() {
        return Err(Error::message("executable missing after install"));
    }
    if !run_status(Command::new(&executable).arg("--version"))?.success() {
        return Err(Error::message("installed executable failed"));
    }

    // Windows Installer registers per-user products under HKCU and may still
    // write the Add/Remove entry to HKLM, so accept either location.
    if arp_entries()?.is_empty() {
        return Err(Error::message("missing uninstall entry"));
    }
    if per_user_products()?.is_empty() {
        return Err(Error::message("product is not registered per user"));
    }

    // The MSI appends the directory with a trailing separator.
    let install_path = install_dir.display().to_string();
    if !path_entries(user_path()?)
        .iter()
        .any(|entry| entry == &install_path)
    {
        return Err(Error::message(format!(
            "PATH does not contain {install_path}"
        )));
    }

    msiexec("MSI uninstall", "/x", &msi, &uninstall_log)?;

    if executable.exists() {
        return Err(Error::message("executable still present after uninstall"));
    }
    if !arp_entries()?.is_empty() {
        return Err(Error::message("uninstall entry still present"));
    }
    if path_entries(user_path()?)
        .iter()
        .any(|entry| entry == &install_path)
    {
        return Err(Error::message("PATH entry was not removed"));
    }

    println!("installer lifecycle ok");
    Ok(())
}

pub fn release(tag: &str, triple: &str) -> Result<()> {
    require_windows("release-msi")?;

    let version = tag.trim_start_matches('v');
    let msi = format!("blockscope-{tag}-{triple}.msi");
    let checksum = format!("{msi}.sha256");

    build(&msi, &format!("target/{triple}/release"), Some(version))?;

    let msi_path = root().join(&msi);
    let mut hasher = Sha256::new();
    hasher.update(fs::read(&msi_path)?);
    let hash = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    fs::write(root().join(&checksum), format!("{hash}  {msi}\n"))?;

    run(Command::new("gh")
        .args(["release", "upload", tag, &msi, &checksum, "--clobber"])
        .current_dir(root()))
}

fn msiexec(action: &str, verb: &str, msi: &Path, log: &Path) -> Result<()> {
    let status = run_status(
        Command::new("msiexec")
            .arg(verb)
            .arg(msi)
            .args(["/qn", "/norestart", "/l*v"])
            .arg(log),
    )?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::message(format!(
            "{action} failed with {}",
            code(status)
        )))
    }
}

fn code(status: ExitStatus) -> i32 {
    status.code().unwrap_or(1)
}

fn arp_entries() -> Result<Vec<registry::Key>> {
    let mut entries = Vec::new();
    for root in UNINSTALL_ROOTS {
        entries.extend(
            registry::query(root)?
                .into_iter()
                .filter(|key| key.value("DisplayName") == Some("Blockscope")),
        );
    }
    Ok(entries)
}

fn per_user_products() -> Result<Vec<registry::Key>> {
    Ok(registry::query(PRODUCTS_KEY)?
        .into_iter()
        .filter(|key| key.value("ProductName") == Some("Blockscope"))
        .collect())
}

fn user_path() -> Result<Option<String>> {
    Ok(registry::query("HKCU\\Environment")?
        .first()
        .and_then(|key| key.value("Path"))
        .map(str::to_owned))
}

fn path_entries(path: Option<String>) -> Vec<String> {
    path.unwrap_or_default()
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.trim_end_matches('\\').to_owned())
        .collect()
}

// A user-scoped .NET SDK lives outside the default location, so both the WiX
// apphost and dotnet tool installs need an augmented PATH (and DOTNET_ROOT for
// the apphost).
struct Toolchain {
    path: std::ffi::OsString,
    dotnet_root: Option<PathBuf>,
}

impl Toolchain {
    fn detect() -> Result<Self> {
        let profile =
            env::var_os("USERPROFILE").ok_or_else(|| Error::message("USERPROFILE is not set"))?;
        let profile = PathBuf::from(profile);
        let dotnet_root = profile.join(".dotnet");

        let mut directories = vec![
            profile.join(".cargo").join("bin"),
            dotnet_root.clone(),
            dotnet_root.join("tools"),
        ];
        directories.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
        let path = env::join_paths(directories)
            .map_err(|error| Error::message(format!("invalid PATH: {error}")))?;

        let dotnet_root =
            if env::var_os("DOTNET_ROOT").is_none() && dotnet_root.join("dotnet.exe").is_file() {
                Some(dotnet_root)
            } else {
                None
            };

        Ok(Self { path, dotnet_root })
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command.env("PATH", &self.path);
        if let Some(dotnet_root) = &self.dotnet_root {
            command.env("DOTNET_ROOT", dotnet_root);
        }
        command
    }

    fn present(&self, program: &str) -> bool {
        let locator = if cfg!(windows) { "where" } else { "which" };
        self.command(locator)
            .arg(program)
            .output()
            .is_ok_and(|output| output.status.success())
    }
}
