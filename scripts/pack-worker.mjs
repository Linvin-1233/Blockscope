#!/usr/bin/env node
// Copy worker-build output into the Cloudflare Build Output layout.
//
// Set ENDPOINT=/api/render to override the render route without editing the
// committed config. ALLOWED_ORIGIN and WORKER_NAME work the same way.

import { cpSync, existsSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const sourceConfig = join(root, "crates/worker/worker.config.json");
const buildDir = process.env.WORKER_BUILD_DIR
  ? join(root, process.env.WORKER_BUILD_DIR)
  : join(root, "crates/worker/build");
const outputRoot = join(root, ".cloudflare/output/v0");
const workerDir = join(outputRoot, "workers/default");
const bundleDir = join(workerDir, "bundle");

if (!existsSync(buildDir)) {
  console.error(`missing worker build output: ${buildDir}`);
  console.error("run: cd crates/worker && worker-build --release");
  process.exit(1);
}

function moduleType(path) {
  if (path.endsWith(".wasm")) return "wasm";
  if (path.endsWith(".js") || path.endsWith(".mjs")) return "esm";
  if (path.endsWith(".cjs")) return "cjs";
  if (path.endsWith(".json")) return "json";
  if (path.endsWith(".txt")) return "text";
  if (path.endsWith(".map")) return "sourcemap";
  return "data";
}

function listFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    if (entry.name === ".gitignore" || entry.name === "package.json") return [];
    const path = join(dir, entry.name);
    return entry.isDirectory() ? listFiles(path) : [path];
  });
}

rmSync(bundleDir, { recursive: true, force: true });
cpSync(buildDir, bundleDir, { recursive: true });
for (const name of [".gitignore", "package.json"]) {
  rmSync(join(bundleDir, name), { force: true });
}

const bundleFiles = listFiles(bundleDir);
const modules = {};
for (const path of bundleFiles) {
  const name = relative(bundleDir, path).split(sep).join("/");
  modules[name] = { type: moduleType(name) };
}

const candidates = ["index.js", "shim.mjs", "worker.mjs"];
const mainModule = candidates.find((name) => modules[name]?.type === "esm")
  ?? Object.keys(modules).find((name) => modules[name].type === "esm");
if (!mainModule) {
  console.error("worker build has no ESM entry module");
  process.exit(1);
}

const config = JSON.parse(readFileSync(sourceConfig, "utf8"));
if (process.env.WORKER_NAME) config.name = process.env.WORKER_NAME;
if (process.env.COMPATIBILITY_DATE) config.compatibilityDate = process.env.COMPATIBILITY_DATE;
if (process.env.ENDPOINT) config.env.ENDPOINT.value = process.env.ENDPOINT;
if (process.env.ALLOWED_ORIGIN) config.env.ALLOWED_ORIGIN.value = process.env.ALLOWED_ORIGIN;
if (process.env.CPU_MS) config.limits = { cpuMs: Number(process.env.CPU_MS) };
config.manifest = { type: "complete", mainModule, modules };

writeFileSync(join(workerDir, "worker.config.json"), `${JSON.stringify(config, null, 2)}\n`);
writeFileSync(
  join(outputRoot, "config.json"),
  `${JSON.stringify({ buildContext: { isPreview: false } }, null, 2)}\n`,
);

const bytes = bundleFiles.reduce((total, path) => total + statSync(path).size, 0);
console.log(`packed ${bundleFiles.length} files (${(bytes / 1024 / 1024).toFixed(1)} MiB)`);
console.log(`entry: ${mainModule}`);
console.log(`endpoint: ${config.env.ENDPOINT.value}`);
console.log(`worker: ${config.name}`);
