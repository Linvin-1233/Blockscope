// Cloudflare Worker build and Cloudflare Build Output packaging.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::process::{cargo, has_command, root, run};

// Workers static assets cap a file at 25 MiB; stay below it so the failure
// happens here instead of at deploy time.
const PACK_LIMIT_BYTES: u64 = 24 * 1024 * 1024;
const ENTRY_CANDIDATES: [&str; 3] = ["index.js", "shim.mjs", "worker.mjs"];

pub fn build() -> Result<()> {
    if !has_command("worker-build") {
        run(cargo().args(["install", "worker-build", "--version", "0.8.7", "--locked"]))?;
    }

    run(Command::new("worker-build")
        .arg("--release")
        .current_dir(root().join("crates/worker")))?;

    pack()
}

pub fn pack() -> Result<()> {
    let root = root();
    let source_config = root.join("crates/worker/worker.config.json");
    let build_dir = match std::env::var_os("WORKER_BUILD_DIR") {
        Some(dir) => root.join(dir),
        None => root.join("crates/worker/build"),
    };
    let output_root = root.join(".cloudflare/output/v0");
    let worker_dir = output_root.join("workers/default");
    let bundle_dir = worker_dir.join("bundle");

    if !build_dir.exists() {
        return Err(Error::message(format!(
            "missing worker build output: {}\nrun: cd crates/worker && worker-build --release",
            build_dir.display()
        )));
    }

    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir)?;
    }
    let mut files = Vec::new();
    copy_tree(&build_dir, &bundle_dir, &mut files)?;

    let modules: BTreeMap<String, &'static str> = files
        .iter()
        .map(|path| (module_name(&bundle_dir, path), module_type(path)))
        .collect();

    let main_module = ENTRY_CANDIDATES
        .iter()
        .find(|name| modules.get(**name).is_some_and(|kind| *kind == "esm"))
        .copied()
        .or_else(|| {
            modules
                .iter()
                .find(|(_, kind)| **kind == "esm")
                .map(|(name, _)| name.as_str())
        })
        .ok_or_else(|| Error::message("worker build has no ESM entry module"))?
        .to_owned();

    let mut config: Value = serde_json::from_str(&fs::read_to_string(&source_config)?)?;
    if let Ok(value) = std::env::var("WORKER_NAME") {
        config["name"] = json!(value);
    }
    if let Ok(value) = std::env::var("COMPATIBILITY_DATE") {
        config["compatibilityDate"] = json!(value);
    }
    if let Ok(value) = std::env::var("ENDPOINT") {
        set_env_value(&mut config, "ENDPOINT", json!(value))?;
    }
    if let Ok(value) = std::env::var("ALLOWED_ORIGIN") {
        set_env_value(&mut config, "ALLOWED_ORIGIN", json!(value))?;
    }
    if let Ok(value) = std::env::var("CPU_MS") {
        let cpu_ms: u64 = value
            .parse()
            .map_err(|_| Error::message(format!("CPU_MS is not a number: {value}")))?;
        config["limits"] = json!({ "cpuMs": cpu_ms });
    }

    let pack_source = match std::env::var_os("RESOURCE_PACK") {
        Some(path) => root.join(path),
        None => root.join("pack.zip"),
    };
    let assets_dir = worker_dir.join("assets");
    if assets_dir.exists() {
        fs::remove_dir_all(&assets_dir)?;
    }

    let mut pack_bytes = 0;
    if pack_source.is_file() {
        pack_bytes = fs::metadata(&pack_source)?.len();
        if pack_bytes > PACK_LIMIT_BYTES {
            return Err(Error::message(format!(
                "resource pack is {:.1} MiB; the Worker limit is 24 MiB",
                mebibytes(pack_bytes)
            )));
        }
        fs::create_dir_all(&assets_dir)?;
        fs::copy(&pack_source, assets_dir.join("pack.zip"))?;
        config["assets"] = json!({ "runWorkerFirst": true });
        set_env_entry(&mut config, "ASSETS", json!({ "type": "assets" }))?;
        set_env_value(&mut config, "RESOURCE_PACK_PATH", json!("/pack.zip"))?;
    } else {
        if let Some(object) = config.as_object_mut() {
            object.remove("assets");
        }
        if let Some(env) = config.get_mut("env").and_then(Value::as_object_mut) {
            env.remove("ASSETS");
        }
        set_env_value(&mut config, "RESOURCE_PACK_PATH", json!(""))?;
        eprintln!(
            "no resource pack at {}; building a color-only Worker",
            pack_source.display()
        );
    }

    if let Ok(path) = std::env::var("RESOURCE_PACK_PATH") {
        if pack_bytes == 0 {
            eprintln!("RESOURCE_PACK_PATH is ignored because no resource pack was bundled");
        } else {
            set_env_value(&mut config, "RESOURCE_PACK_PATH", json!(path))?;
        }
    }

    let modules: Map<String, Value> = modules
        .iter()
        .map(|(name, kind)| (name.clone(), json!({ "type": kind })))
        .collect();
    config["manifest"] = json!({
        "type": "complete",
        "mainModule": main_module,
        "modules": modules,
    });

    fs::create_dir_all(&worker_dir)?;
    write_json(&worker_dir.join("worker.config.json"), &config)?;
    write_json(
        &output_root.join("config.json"),
        &json!({ "buildContext": { "isPreview": false } }),
    )?;

    let bytes = files
        .iter()
        .filter_map(|path| fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    println!("packed {} files ({:.1} MiB)", files.len(), mebibytes(bytes));
    println!("entry: {main_module}");
    println!("endpoint: {}", env_text(&config, "ENDPOINT"));
    println!(
        "{}",
        if pack_bytes > 0 {
            format!(
                "resource pack: {:.1} MiB at {}",
                mebibytes(pack_bytes),
                env_text(&config, "RESOURCE_PACK_PATH")
            )
        } else {
            "resource pack: none (color-only)".to_owned()
        }
    );
    println!("worker: {}", config["name"].as_str().unwrap_or(""));

    Ok(())
}

// Copies a directory tree, skipping the build system's own metadata files: they
// are not Worker modules and would confuse the deploy.
fn copy_tree(source: &Path, target: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    fs::create_dir_all(target)?;

    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let name = entry.file_name();
        if name == ".gitignore" || name == "package.json" {
            continue;
        }

        let from = entry.path();
        let to = target.join(&name);
        if entry.file_type()?.is_dir() {
            copy_tree(&from, &to, files)?;
        } else {
            fs::copy(&from, &to)?;
            files.push(to);
        }
    }

    Ok(())
}

fn module_name(bundle: &Path, path: &Path) -> String {
    path.strip_prefix(bundle)
        .unwrap_or(path)
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn module_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("wasm") => "wasm",
        Some("js" | "mjs") => "esm",
        Some("cjs") => "cjs",
        Some("json") => "json",
        Some("txt") => "text",
        Some("map") => "sourcemap",
        _ => "data",
    }
}

fn env_text<'a>(config: &'a Value, name: &str) -> &'a str {
    config["env"][name]["value"].as_str().unwrap_or("")
}

fn env_object(config: &mut Value) -> Result<&mut Map<String, Value>> {
    config
        .get_mut("env")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| Error::message("worker.config.json has no env object"))
}

fn set_env_entry(config: &mut Value, name: &str, entry: Value) -> Result<()> {
    env_object(config)?.insert(name.to_owned(), entry);
    Ok(())
}

fn set_env_value(config: &mut Value, name: &str, value: Value) -> Result<()> {
    let entry = env_object(config)?.entry(name).or_insert_with(|| json!({}));
    let entry = entry
        .as_object_mut()
        .ok_or_else(|| Error::message(format!("worker.config.json env.{name} is not an object")))?;
    entry.insert("value".to_owned(), value);
    Ok(())
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(value)?;
    fs::write(path, format!("{text}\n"))?;
    Ok(())
}

fn mebibytes(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0
}
