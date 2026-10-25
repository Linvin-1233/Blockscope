# Phase 1 Task List — Rust Litematic Parser + CLI

**Status:** Active  
**Plan:** `tasks/plan.md`

Do not overwrite this list while tasks remain unchecked. Update checkboxes in
place and record verification results next to each checkpoint.

## Task 1: Project housekeeping

**Description:** Align the repository rules and ignore rules with the chosen
Go/Rust/Windows/Linux setup before adding code.

**Acceptance criteria:**

- [x] `AGENTS.md` uses per-language formatting rules:
  - Go: `gofmt`/`goimports`, tabs, PascalCase exports.
  - Rust: `rustfmt`, 4 spaces, snake_case functions, PascalCase types.
  - JS/TS: Prettier, 120 columns, camelCase.
  - `//` comments, no emoji, 800-line soft limit.
- [x] `AGENTS.md` points to `.agents/skills/` instead of `.opencode/skills/`.
- [x] `.gitignore` tracks `.docs/`, ignores `.idea/`, `target/`,
      `resourcepacks/`, private fixtures, renders, and cache.
- [x] `README.md` links the design docs and states the current phase.
- [x] `skills-lock.json` is tracked.

**Verification:**

- [x] `git check-ignore -v .docs/blockscope-engineering-design.md`
      returns non-zero (not ignored).
- [x] `git status --short` shows only intended changes.
- [x] No secrets or private litematic files are staged.

**Dependencies:** None  
**Files likely touched:** `AGENTS.md`, `.gitignore`, `README.md`,
`skills-lock.json`  
**Estimated scope:** Small

## Task 2: Rust workspace and quality gates

**Description:** Create the workspace skeleton and the commands every later
task will run.

**Acceptance criteria:**

- [x] Root `Cargo.toml` defines workspace members:
      `crates/nbt`, `crates/scene`, `crates/litematic`, `crates/cli`.
- [x] `rust-toolchain.toml` pins stable Rust.
- [x] `rustfmt.toml` sets `max_width = 100` and edition 2024.
- [x] Each crate has a minimal `Cargo.toml` and `src/lib.rs` or `src/main.rs`.
- [x] `scripts/check.ps1` and `scripts/check.sh` run format, clippy, and tests.
- [x] `cargo run -p blockscope-cli -- --help` prints usage.
- [x] `.github/workflows/ci.yml` runs the same gates on Ubuntu and Windows.

**Verification:**

- [x] `rustup default stable` completed and `rustc --version` works.
- [x] `cargo check --workspace` passes.
- [x] `powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1`
      passes on Windows. `pwsh` is not installed locally.
- [x] `bash ./scripts/check.sh` passes on Linux. Verified in WSL Ubuntu 24.04
      with `CARGO_TARGET_DIR=/tmp/litematic-target`.

**Dependencies:** Task 1  
**Files likely touched:** root workspace files, four crate manifests, two
scripts  
**Estimated scope:** Medium

## Task 3: NBT types, limits, and errors

**Description:** Define the NBT value model, decode budget, and stable error
codes without implementing the full decoder yet.

**Acceptance criteria:**

- [x] `NbtValue` covers compound, list, byte, short, int, long, float, double,
      string, byte array, int array, and long array.
- [x] `DecodeLimits` covers input bytes, decompressed bytes, depth, tag count,
      string bytes, list length, array length, and total allocation.
- [x] `NbtError` exposes stable codes:
      `INVALID_TAG`, `TRUNCATED_INPUT`, `INVALID_COMPRESSION`,
      `NBT_LIMIT_EXCEEDED`.
- [x] Unit tests cover limit arithmetic without allocating the full budget.

**Verification:**

- [x] `cargo test -p litematic-nbt` passes.
- [x] Error display includes code and byte offset.

**Dependencies:** Task 2  
**Files likely touched:** `crates/nbt/src/lib.rs`, `value.rs`, `limits.rs`,
`error.rs`  
**Estimated scope:** Small

## Task 4: NBT decoder and tests

**Description:** Implement the big-endian NBT decoder with gzip, zlib, and
uncompressed input detection.

**Acceptance criteria:**

- [x] Decodes all tag types, including nested compounds and lists.
- [x] Detects gzip and zlib by magic bytes; invalid streams return
      `INVALID_COMPRESSION`. Other magic falls back to uncompressed NBT.
- [x] Enforces every `DecodeLimits` value before allocation.
- [x] Rejects trailing bytes after the root tag.
- [x] Rejects negative lengths, integer overflow, and invalid UTF-8.
- [x] Handles truncated input without panic.

**Verification:**

- [x] `cargo test -p litematic-nbt` passes.
- [x] Deterministic truncation loop covers every prefix of a valid NBT file;
      `proptest`/fuzz is scheduled in Task 12.
- [x] `cargo clippy -p litematic-nbt --all-targets -- -D warnings` passes.

**Dependencies:** Task 3  
**Files likely touched:** `crates/nbt/src/decode.rs`, `compression.rs`,
`tests/`  
**Estimated scope:** Medium

## Checkpoint 1: NBT layer

- [x] `cargo test -p litematic-nbt` passes.
- [x] Truncated, oversized, and deeply nested inputs fail without panic.
- [x] Compression detection is covered by tests.
- [ ] Review with human before proceeding.

## Task 5: Litematic container, metadata, regions

**Description:** Parse the litematic top level and region headers for v1-v7.

**Acceptance criteria:**

- [x] Reads `Version`, `MinecraftDataVersion`, `Metadata`, and `Regions`.
- [x] Reads region `Position`, `Size`, `BlockStatePalette`, `BlockStates`,
      `TileEntities`, `Entities`, and pending tick lists.
- [x] Normalizes negative region sizes into min corner plus positive size.
- [x] Rejects unsupported versions and invalid bounds with stable codes.
- [x] Records region order for deterministic overlap handling.
- [x] Documents the verified version differences in `docs/litematic-format.md`.

**Verification:**

- [x] Synthetic fixtures for v1, v6, and v7 parse. v1 is covered in
      `region_data.rs`; v6/v7 in the golden manifests.
- [x] Negative-size bounds tests pass.
- [x] Invalid version and zero/overflow bounds tests pass.
- [x] `cargo test -p litematic-core` passes.

**Dependencies:** Task 4  
**Files likely touched:** `crates/litematic/src/container.rs`, `region.rs`,
`version.rs`, `tests/`  
**Estimated scope:** Medium

## Task 6: Palette and packed block states

**Description:** Decode palette entries and packed long indices into packed
storage.

**Acceptance criteria:**

- [x] Palette entries use canonical `namespace:block` plus sorted properties.
- [x] Bits per entry follows the litematic format; entries crossing long
      boundaries are handled.
- [x] Palette index bounds are checked for every block.
- [x] Empty and all-air regions are supported.
- [x] Storage stays packed; no per-block heap object is created. Consumers
      decode pages or sequential runs.
- [x] Bit widths at 1, 2, 16, 17, 32, 33, and 64 are covered.

**Verification:**

- [x] Boundary tests pass for every listed bit width.
- [x] Invalid palette index returns `INVALID_PALETTE_INDEX`.
- [x] Data length mismatch returns `BLOCK_STATE_DATA_LENGTH_MISMATCH`.
- [x] Packed storage is reviewed to avoid per-block heap objects; property
      tests cover round trips across long boundaries.

**Dependencies:** Task 5  
**Files likely touched:** `crates/litematic/src/palette.rs`,
`packed_states.rs`, `paging.rs`, `tests/`  
**Estimated scope:** Medium

## Task 7: Block entities and diagnostics

**Description:** Extract block-entity outer data and record diagnostics without
tying the parser to a renderer.

**Acceptance criteria:**

- [x] Each block entity keeps its type id, position, and raw NBT payload.
- [x] Positions outside the region bounds produce a diagnostic and are dropped.
- [x] Block-entity count is limited; total payload bytes are bounded by
      `DecodeLimits`.
- [x] Unknown block-entity types are preserved and counted.
- [x] Diagnostics have stable codes and severity.

**Verification:**

- [x] Chest and generic block-entity fixtures preserve type, position, and
      NBT; signs use the same path.
- [x] Out-of-bounds position test follows the documented policy.
- [x] Block-entity count limit returns `BLOCK_ENTITY_LIMIT_EXCEEDED`; payload
      is bounded by `DecodeLimits`.

**Dependencies:** Task 5  
**Files likely touched:** `crates/litematic/src/block_entity.rs`,
`diagnostics.rs`, `tests/`  
**Estimated scope:** Medium

## Task 8: Scene IR and statistics

**Description:** Build the canonical scene representation and compute material
statistics.

**Acceptance criteria:**

- [x] `Scene` contains bounds, regions, block entities, statistics, and
      diagnostics.
- [x] Region overlap order is deterministic and documented.
- [x] Statistics separate air, non-air, block entities, and per-state counts.
- [x] Counts sum to the validated volume.
- [x] Block-state keys are sorted and deduplicated for output.
- [x] Unknown/placeholder classification is left to the catalog layer.

**Verification:**

- [x] Statistics match an independent count on synthetic fixtures.
- [x] Multi-region and negative-size fixtures produce correct bounds.
- [x] Overlap fixture produces a diagnostic and a stable winner.

**Dependencies:** Task 6, Task 7  
**Files likely touched:** `crates/scene/src/lib.rs`, `scene.rs`, `statistics.rs`,
`region.rs`, `tests/`  
**Estimated scope:** Medium

## Checkpoint 2: Parser and stats

- [x] Synthetic litematic fixtures parse into Scene IR.
- [x] Statistics sum to the validated volume.
- [x] Negative sizes and multi-region bounds are covered.
- [ ] Review with human before proceeding.

## Task 9: Hashes and manifest JSON

**Description:** Compute input and scene hashes and emit the versioned manifest.

**Acceptance criteria:**

- [x] `inputHash` is SHA-256 over the raw file bytes.
- [x] `sceneHash` is SHA-256 over a canonical serialization of Scene IR.
- [x] Manifest includes parser version, litematic version, bounds, region count,
      block counts, block-entity counts, diagnostics, and hashes.
- [x] JSON field order and block-state order are stable.
- [x] Same input and parser build produce identical manifest bytes.

**Verification:**

- [x] Two runs on the same fixture produce byte-identical JSON.
- [x] Changing one block changes `sceneHash`; `inputHash` remains the raw-input
      hash.
- [x] Manifest schema test covers the required fields.

**Dependencies:** Task 8  
**Files likely touched:** `crates/scene/src/hash.rs`, `manifest.rs`,
`tests/`  
**Estimated scope:** Small

## Task 10: `blockscope` CLI

**Description:** Expose the parser through `blockscope inspect`, `validate`, and
`stats`.

**Acceptance criteria:**

- [x] `blockscope inspect <file> --json` prints the manifest.
- [x] `blockscope validate <file>` prints a summary or structured error.
- [x] `blockscope stats <file>` prints sorted material counts.
- [x] Exit codes distinguish success, invalid input, and internal error.
- [x] Errors contain stable codes and no stack traces by default.
- [x] `--help` documents all commands and limits.

**Verification:**

- [x] CLI runs on every synthetic fixture.
- [x] CLI integration tests cover JSON and human-readable output.
- [x] Invalid file returns exit code 2 and a stable error code.
- [x] `cargo run -p blockscope-cli -- inspect --help` works.

**Dependencies:** Task 9  
**Files likely touched:** `crates/cli/src/main.rs`, `commands.rs`,
`tests/`  
**Estimated scope:** Medium

## Checkpoint 3: CLI end-to-end

- [x] `blockscope inspect <file> --json` emits a stable manifest.
- [x] `blockscope validate` returns stable error codes.
- [x] `blockscope stats` prints deterministic counts.
- [ ] Review with human before proceeding.

## Task 11: Fixtures and golden manifests

**Description:** Build the synthetic fixture corpus and golden outputs without
committing private or Minecraft assets.

**Acceptance criteria:**

- [x] `fixtures/litematic/README.md` explains how to drop in real samples.
- [x] Synthetic fixtures cover v1/v6/v7, multi-region, negative sizes, all-air
      regions, block entities, and bit-width boundaries.
- [x] Golden manifests are committed under `fixtures/golden/`.
- [x] Real-file tests are opt-in through `LITEMATIC_FIXTURES_DIR`.
- [x] Tests skip cleanly when no real files are present.
- [x] No `.litematic` files are committed without explicit permission.

**Verification:**

- [x] `cargo test --workspace` passes with no real files.
- [ ] `LITEMATIC_FIXTURES_DIR=... cargo test --workspace -- --ignored`
      runs the real-file smoke tests. The test is implemented but requires the
      user's local fixtures.
- [x] `git status --short` shows no private fixtures.

**Dependencies:** Task 8, Task 10  
**Files likely touched:** `fixtures/litematic/README.md`,
`fixtures/golden/*.json`, `crates/litematic/tests/`  
**Estimated scope:** Medium

## Task 12: Format documentation and hardening

**Description:** Document verified format behavior and add the remaining
property/fuzz coverage.

**Acceptance criteria:**

- [x] `docs/litematic-format.md` covers v1-v7 fields, negative sizes, packing,
      block entities, region overlap, and known edge cases.
- [x] Claims are tied to a fixture, upstream source, or an explicitly marked
      open item.
- [x] Property tests cover packed-index round trips and NBT truncation.
- [x] Property/fuzz smoke test runs for a bounded time without crash.
- [x] Phase 1 acceptance commands pass on Windows and Linux.

**Verification:**

- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes on Windows and in WSL Ubuntu 24.04.
- [ ] `docs/litematic-format.md` reviewed by human.

**Dependencies:** Task 10, Task 11  
**Files likely touched:** `docs/litematic-format.md`, `crates/litematic/tests/`,
`crates/nbt/tests/`  
**Estimated scope:** Medium

## Checkpoint 4: Phase 1 acceptance

- [x] `cargo fmt --all --check` passes.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [x] `cargo test --workspace` passes on Windows and in WSL Ubuntu 24.04.
- [x] No Minecraft assets or private fixtures are committed.
- [x] `docs/litematic-format.md` records verified v1-v7 behavior.
- [x] Demo: `blockscope inspect`, `blockscope validate`, and `blockscope stats` on
      synthetic fixtures, covered by CLI integration tests.
- [ ] Human review and approval to start Phase 2.

## Release distribution: prebuilt binaries

**Status:** Done for prebuilt binaries. crates.io publishing is intentionally
out of scope while the public Rust API is unstable and the repository has no
`LICENSE` file; the stable integration surface is the CLI and engine protocol
v1.

**Acceptance criteria:**

- [x] `.github/workflows/release.yml` creates the GitHub Release from
      `CHANGELOG.md` on a `v*` tag.
- [x] A tag that does not match `[workspace.package] version` fails before the
      release is created.
- [x] Windows x86_64, Linux x86_64, and Linux aarch64 archives are uploaded with
      `.sha256` checksums.
- [x] Windows artifacts link the static CRT; Linux artifacts are static musl.
- [x] Manual dispatch builds the same targets with `dry-run`, so the workflow
      can be verified without tagging.
- [x] Installation docs prefer prebuilt binaries and mark source builds as a
      development path.

**Verification:**

- [x] `npx js-yaml .github/workflows/release.yml` parses the workflow.
- [x] `cargo build --release --locked -p blockscope-cli --bin blockscope`
      passes.
- [x] The same build passes with `RUSTFLAGS=-C target-feature=+crt-static`, and
      the resulting executable imports neither `VCRUNTIME140.dll` nor
      `api-ms-win-crt-*`.
- [x] `blockscope --version` prints `blockscope 0.1.0`.
- [x] `parse-changelog CHANGELOG.md 0.1.0` and `parse-changelog CHANGELOG.en.md 0.1.0`
      both succeed, so the release body is non-empty.
- [x] `workflow_dispatch` run is green on GitHub Actions.
- [x] The `v0.1.0` tag uploaded the three archives and a changelog body.

## Release distribution: Windows MSI

**Status:** Shipped in 0.1.1; per-user scope and the install/uninstall lifecycle
were fixed in 0.1.2.

**Acceptance criteria:**

- [x] `installer/blockscope.wxs` installs per user under
      `%LOCALAPPDATA%\Programs\Blockscope` and maintains the user `PATH`.
- [x] `scripts/build-msi.ps1` pins WiX 6.0.2 and rejects pre-release versions.
- [x] CI builds the MSI on every push and uploads it as a validation artifact.
- [x] The release workflow uploads the MSI and a `.sha256` file for `v*` tags.
- [x] Windows installation docs prefer the MSI and keep the portable zip.

**Verification:**

- [x] `npx js-yaml` parses both workflows; `installer/blockscope.wxs` is
      well-formed XML.
- [x] The CI `installer` job builds the MSI successfully.
- [x] CI verifies `ALLUSERS=2` / `MSIINSTALLPERUSER=1`, the per-user product
      registration, and runs install, `blockscope --version`, uninstall entry,
      `PATH`, and uninstall cleanup.

## Platform support: macOS

**Status:** Done.

**Acceptance criteria:**

- [x] Process RSS detection implemented for macOS via `proc_pidinfo` in `crates/cli/src/supervise.rs`.
- [x] Memory limit and capacity detection implemented for macOS via `sysctl` and Mach VM statistics in `crates/cli/src/capacity.rs`.
- [x] Peak RSS detection implemented for macOS via `getrusage` in `crates/cli/src/commands/bench.rs`.
- [x] Full test suite passes on macOS, including `supervisor_enforces_the_memory_ceiling`.
- [x] CI workflow tests on `macos-latest`.
- [x] Release workflow packages `aarch64-apple-darwin` and `x86_64-apple-darwin` tarballs with checksums.
- [x] User and technical documentation updated for macOS.

**Verification:**

- [x] `cargo test --workspace` passes 221 tests on macOS.
- [x] `cargo run -q --bin blockscope -- capacity --json` outputs `platform: macos` with real memory metrics.
- [x] `cargo build --target x86_64-apple-darwin --bin blockscope` builds x86_64 binary on macOS.
- [x] `npx js-yaml .github/workflows/ci.yml` and `release.yml` pass validation.
- [x] `bash scripts/check.sh` passes format, clippy with `-D warnings`, and test suite.

## Feature: Terminal image rendering protocols

**Status:** Done.

**Acceptance criteria:**

- [x] Implement Kitty graphics protocol encoder (`a=T,f=100`, chunking, dimension constraints, quiet mode).
- [x] Implement iTerm2 inline image protocol encoder (`OSC 1337;File=inline=1`, size, aspect ratio, width/height).
- [x] Implement DEC Sixel graphics protocol encoder with color quantization, palette generation, and RLE compression.
- [x] Implement universal Halfblocks (Unicode ▀ + ANSI 24-bit Truecolor) raster text renderer with color caching.
- [x] Support automatic terminal protocol and dimension detection across Linux, macOS, and Windows.
- [x] Support Windows (PowerShell, Windows Terminal, conhost) with `ENABLE_VIRTUAL_TERMINAL_PROCESSING` console mode initialization and buffer info queries.
- [x] CLI flags added to `blockscope render`: `--terminal` (`-t`), `--terminal-protocol`, `--terminal-width`, `--terminal-height`, and `--no-file`.
- [x] CLI aliases `preview` and `term` added for intuitive terminal previewing.

**Verification:**

- [x] `cargo test --workspace` passes 262 tests.
- [x] Unit and integration tests cover all protocols in `crates/render/tests/terminal.rs` and `crates/cli/tests/cli.rs`.
- [x] `cargo fmt --all --check` passes without diffs.
- [x] `cargo clippy --workspace --all-targets -- -D warnings` passes without warnings.
- [x] `bash scripts/check.sh` passes full validation suite.
