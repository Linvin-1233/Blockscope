# Implementation Plan: Phase 1 — Rust Litematic Parser + CLI

**Status:** Proposed  
**Date:** 2026-10-24  
**Scope:** Phase 1 only. Go/QQ service and the renderer are out of scope.

## Overview

Build a standalone Rust workspace that parses `.litematic` v1-v7 into a canonical
Scene IR, computes material statistics and diagnostics, and exposes a CLI for
validation and JSON manifests. This freezes the input contract before the
renderer and QQ service are added.

## Confirmed Decisions

- Start with the Rust parser and CLI. No Go, QQ, or renderer in Phase 1.
- Support litematic v1-v7.
- The `Standalone-Litematic-Renderer` repository is study-only. Do not copy its
  source, textures, fonts, or models.
- Default rendering catalog is color-block based and requires no Minecraft
  assets. Resource packs are optional and supplied by the deployer; they are
  never committed to this repository.
- Production targets both Linux and Windows.
- Engineering rules: per-language formatters, `//` comments, no emoji, and a
  soft 800-line file limit.

## Architecture Decisions

### ADR-001: Single cross-platform `main` branch

Keep one cross-platform codebase. CI runs on Windows and Linux. OS-specific
release branches (`release/windows`, `release/linux`) are only for packaging and
service scripts; core crates do not fork. If a platform dependency forces a
difference, isolate it behind `cfg` or a small adapter crate.

**Rationale:** separate branches for parser/renderer code would duplicate the
highest-risk logic and make fixes diverge.

### ADR-002: Color-block default, resource pack optional

The default `BlockCatalog` maps block states to original colors and built-in
geometry. No Minecraft assets are required to run or test the project. A
resource-pack catalog can be added later against a user-provided directory.
Phase 1 only defines `BlockStateKey` and the catalog boundary; it does not load
resource packs.

**Rationale:** the service must run on a clean machine without distributing
Minecraft assets, and the parser can be validated before the asset pipeline
exists.

### ADR-003: CLI-first parser boundary

`renderd inspect`, `renderd validate`, and `renderd stats` are the acceptance
surface for Phase 1. The Go/Rust HTTP boundary is deferred until the Scene IR
and manifest schema are stable.

**Rationale:** a CLI gives deterministic, scriptable tests without introducing
the service lifecycle early.

### ADR-004: Study-only reference policy

Use the public Litematica upstream (LGPL-3.0) for format semantics. Write
independent Rust code; do not translate or copy Java source. The unlicensed
`Standalone-Litematic-Renderer` is used only to observe behavior and compare
outputs locally. Record provenance in `docs/litematic-format.md`.

**Rationale:** the reference repository has no license, so copying code or
assets would create redistribution risk.

### ADR-005: Packed storage with page decode

Keep palette indices in the original tightly packed long array instead of
per-block heap objects. Consumers decode fixed-size pages or sequential runs.
Avoid duplicating the full structure as a second dense array.

**Rationale:** large projections must stay within a predictable memory budget.

### ADR-006: Fail closed at the input boundary

Every size, depth, count, coordinate, and multiplication check uses checked
arithmetic. Unsupported or malformed input returns a structured error and
produces no partial scene.

**Rationale:** `.litematic` is untrusted input; partial parsing would leak
inconsistent state into later stages.

### ADR-007: Deterministic manifest

Sort block-state keys, preserve file region order, use fixed JSON field order,
and compute SHA-256 hashes. The same input and parser build must produce the
same manifest bytes.

**Rationale:** deterministic output is required for golden tests, caching, and
diagnosis.

## Scope

### In scope

- NBT decoding with compression detection and resource limits.
- Litematic v1-v7 metadata, regions, negative sizes, palettes, packed states,
  and block-entity outer data.
- Canonical Scene IR, bounds, region order, diagnostics, statistics, and hashes.
- CLI for inspect/validate/stats with JSON output and stable error codes.
- Synthetic fixtures, golden manifests, property tests, and format notes.

### Out of scope

- Go `qqbotd`, QQ SDK, job API, queue, and result delivery.
- CPU rasterization, camera, lighting, and PNG output.
- Resource pack loading, texture atlases, and block models.
- Block-entity typed rendering (chests, signs, banners, skulls).
- GIF output, GPU backend, and multi-worker scheduling.

## Task Index

| ID | Task | Depends on | Size |
|---|---|---|---|
| T1 | Project housekeeping | None | S |
| T2 | Rust workspace and quality gates | T1 | M |
| T3 | NBT types, limits, and errors | T2 | S |
| T4 | NBT decoder and tests | T3 | M |
| T5 | Litematic container, metadata, regions | T4 | M |
| T6 | Palette and packed block states | T5 | M |
| T7 | Block entities and diagnostics | T5 | M |
| T8 | Scene IR and statistics | T6, T7 | M |
| T9 | Hashes and manifest JSON | T8 | S |
| T10 | `renderd` CLI | T9 | M |
| T11 | Fixtures and golden manifests | T8, T10 | M |
| T12 | Format documentation and hardening | T10, T11 | M |

Detailed acceptance criteria and verification steps live in
`tasks/todo.md`.

## Checkpoints

### Checkpoint 1: NBT layer

- [ ] `cargo test -p litematic-nbt` passes.
- [ ] Truncated, oversized, and deeply nested inputs fail without panic.
- [ ] Compression detection is covered by tests.

### Checkpoint 2: Parser and stats

- [ ] Synthetic litematic fixtures parse into Scene IR.
- [ ] Statistics sum to the validated volume.
- [ ] Negative sizes and multi-region bounds are covered.

### Checkpoint 3: CLI end-to-end

- [ ] `renderd inspect <file> --json` emits a stable manifest.
- [ ] `renderd validate` returns stable error codes.
- [ ] `renderd stats` prints deterministic counts.

### Checkpoint 4: Phase 1 acceptance

- [ ] `cargo fmt --all --check` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `cargo test --workspace` passes on Windows and Linux.
- [ ] No Minecraft assets or private fixtures are committed.
- [ ] `docs/litematic-format.md` records verified v1-v7 behavior.

## Risks and Mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Rust toolchain is installed but no default version is configured | Cannot build | Run `rustup default stable` and pin it in `rust-toolchain.toml` |
| No real litematic samples in the repository | Parser may miss real-world edge cases | Synthetic fixtures plus user-side validation with `renderd inspect` |
| Litematic v1-v7 differences are undocumented | Silent misparse | Read Litematica upstream, write format notes, add boundary tests |
| Windows/Linux path and atomic rename differences | Publish or cleanup failures | CI matrix, path tests, retry-safe atomic publish in later phases |
| Reference repository has no license | Legal and contamination risk | Study-only policy, no code/asset copying, provenance notes |
| Color-block default does not meet visual expectations | Product mismatch | Clearly label it as a fallback; optional resource-pack catalog later |
| Large structures exceed memory budget | OOM | Paged storage, decode limits, two-stage admission in later phases |

## Open Questions

1. Region overlap policy: first region wins or later region wins? Confirm against
   Litematica behavior and document in T5.
2. Exact v1-v7 differences: resolve in T5/T12 with the upstream format source
   and synthetic boundary fixtures.
3. Block-entity typed extraction: Phase 1 keeps raw NBT plus type/position;
   typed container and sign data moves to Phase 2. Confirm after parser output
   is reviewed.
4. Default color palette and partial-block shape coverage: Phase 2 design.
5. CI provider: assume GitHub Actions. If unavailable, use local scripts plus
   WSL for the Linux check.

## Prerequisites

- Approve `rustup default stable` (Rustup exists at
  `C:\Users\weizh\.cargo\bin`, but no default toolchain is selected).
- Go 1.25+ is not needed until Phase 4.
- WSL is available for a local Linux check; GitHub Actions is preferred for CI.
- Real `.litematic` files can be dropped into `fixtures/litematic/`; tests skip
  them when the directory is empty.
