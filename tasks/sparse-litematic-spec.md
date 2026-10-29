# Spec: `sparse-litematic`

**Module id:** `sparse-litematic`
**Status:** Spike passed; integration plan required before implementation
**Depends on:** `memory-baseline`

## Objective

Make litematic parse and scene memory scale with occupied blocks for sparse
regions, instead of with the bounding-box volume. A `512 x 512 x 512` region
holding three stone blocks should not allocate tens of megabytes of packed
state just because most of its cells are empty.

The change must be output-compatible: same scene hash, manifest, PNG pixels,
diagnostics, limits, and error codes. It is an internal representation and
reader change, not a rendering change.

## Problem

The current pipeline:

1. decompresses the whole gzip member into a `Vec<u8>`;
2. builds a full NBT tree;
3. moves each `BlockStates` `LongArray` into `RawRegion`;
4. moves it again into `PackedIndices` in `Region`.

Two costs follow from the bounding box alone:

- the decompressed NBT and the `LongArray` coexist for the whole parse;
- `BlockStates` is `volume * bits / 64` longs, even when almost every value is
  the same air/default entry.

The mesh and projected-quad tables dominate the 9.4M-block dense exact render,
so this module does **not** target that 18.9 GB peak. It targets large sparse
bounding boxes and oversized decompressed NBT.

## Measured baseline (2026-10-28, 48 GB Windows host)

These numbers were measured on the current release binary before writing any
implementation. They change the priority of this module.

| Case                                                  | Peak working set                          |
| ----------------------------------------------------- | ----------------------------------------- |
| `validate 000001` (parse + scene, 9.39M blocks)       | 108 MB                                    |
| `validate 000002` (parse + scene, 50.65M blocks)      | 132 MB                                    |
| `validate 000003` (parse + scene, 67.1M blocks)       | 130 MB                                    |
| `validate 000004` (parse + scene, 67.1M blocks)       | 115 MB                                    |
| `render 000001 --stream-mesh --force-limits`          | 407 MB                                    |
| `bench 000001 --force-limits` (full mesh, color-only) | 6.8 GB                                    |
| synthetic sparse `1024^3`, 1 block, `--force-limits`  | 495 MB                                    |
| synthetic sparse `512^3`, 1 block                     | rejected: region volume 134M > 100M limit |

The 18.9 GB figure in `docs/rendering.md` is the client-jar full-mesh path, not
the streamed render path. `--stream-mesh` already keeps the measured real
fixture at about 407 MB, and parse+scene stays around 100-130 MB.

The sparse representation only becomes relevant when the caller raises the
volume limit: a `512^3` region is rejected by default, and `1024^3` needs
`--force-limits`. At the default 100M volume cap, the dense 2-bit array is about
25 MB, so a rewrite is not justified by the current fixtures alone.

**S0 result (2026-10-28):** go for an integration plan. The standalone probe in
`crates/litematic/examples/sparse_probe.rs` kept a constant 142 KB allocation
peak while the dense parser needed 100.7 MB for a sparse `512^3` and 805.3 MB
for a sparse `1024^3`. The sparse decode took about 2x the dense parse time
(97.7 ms vs 48.4 ms and 689.5 ms vs 354.6 ms), within the threshold. The probe
does not build a scene or validate palette indices, so production integration
still needs `RegionStates`, canonical hashing, parity tests, and error-path
coverage. Do not start it without a new implementation plan.

## Tech Stack

- Rust 2024, `litematic-core`, `litematic-nbt`, `litematic-scene`, `litematic-mesh`.
- `flate2` is already used by `litematic-nbt`; prefer extending that crate's
  reader API over adding a second decompressor to `litematic-core`.
- No GPU, no new format support, no external service.

## Commands

```bash
# Focused parser/scene tests
cargo test -p litematic-core
cargo test -p litematic-scene
cargo test -p litematic-mesh

# Memory oracle from memory-baseline
cargo test -p litematic-core --test memory -- --nocapture

# Output parity and performance
cargo test -p blockscope-cli --test golden
cargo run --release -p blockscope-cli -- bench <fixture.litematic>

# Full gate
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Project Structure

```text
crates/litematic/src/            # streaming reader, compact construction
crates/litematic-nbt/src/        # bounded streaming NBT reader (if needed)
crates/scene/src/region.rs       # RegionStates
crates/scene/src/sparse.rs       # SparseIndices
crates/scene/src/hash.rs         # canonical packed-word hashing
crates/scene/src/statistics.rs   # sparse counts
crates/mesh/src/view.rs          # sparse page occupancy
crates/cli/src/runtime.rs        # compact parse selection and fallback
docs/performance.md              # measured before/after
```

## Code Style

- Keep identifiers English; comments may be Chinese for domain intent.
- Prefer explicit methods over trait objects; the hot paths stay monomorphized.
- No `unwrap` on untrusted input. Return `LitematicError` with the existing
  codes.
- Keep files under 800 lines; split the streaming reader into modules.

## Design

### 1. Scene representation

Introduce `RegionStates`:

```rust
pub enum RegionStates {
    Dense(PackedIndices),
    Sparse(SparseIndices),
}
```

`SparseIndices` stores:

- the region volume;
- a default palette id;
- a sorted `Vec<(u64 index, u32 palette_id)>` for every value that differs from
  the default.

Methods: `volume`, `bits`, `get`, `iter_occupied`, `counts`, `heap_bytes`, and
`hash_canonical_words`.

`Region.states` becomes `RegionStates`. `PackedIndices` stays public and remains
the dense constructor used by tests, LOD, and fixtures. Add `From<PackedIndices>`
and a dense `RegionStates::from_values` helper so existing callers migrate with
minimal churn.

Sparse is only chosen when its estimated bytes are lower than the dense array;
dense files keep `PackedIndices` and the current fast path.

### 2. Two-pass reader

Add a bounded streaming reader, preferably in `litematic-nbt`:

- Pass 1 decompresses and scans the NBT structure. It records metadata, region
  names/positions/sizes/palettes, block entity list offsets, and the byte offset
  and length of every `BlockStates` `LongArray`. It does not retain the arrays.
- While scanning `BlockStates`, count values that differ from the likely
  default. This gives the density needed to choose dense or sparse without a
  third pass. If counting in pass 1 is too expensive, use a two-pass
  `BlockStates` scan and document the cost.
- Pass 2 re-decompresses, seeks to the recorded offsets, and decodes the
  `LongArray` in bounded chunks directly into `Dense` or `Sparse`.

The reader must preserve current behavior for:

- gzip, zlib, and uncompressed input;
- truncated input and `ignore_block_entity_nbt`;
- trailing bytes, NBT depth/tag/string/array limits;
- expected `BlockStates` length and palette-index validation;
- multiple gzip members and the existing fallback semantics.

If the streaming reader cannot match a case, fall back to the existing dense
parser rather than changing the error.

### 3. Integration

- `Region::volume` and manifest volume stay the source volume.
- `scene_hash` hashes the canonical litematic packed words. For sparse states,
  generate the words incrementally into SHA-256 without materializing the dense
  array. Preserve the original final-word padding bits, or document and test a
  canonical-zero policy.
- `statistics` uses `counts()` so a sparse region is O(occupied), not O(volume).
- `mesh::view` uses `iter_occupied` for sparse page occupancy; dense keeps the
  current sequential scan.
- `mesh::page` and `lod` keep using `get(index)`. Sparse `get` is a binary search
  over occupied entries; it is only used when the region is sparse.
- LOD output stays dense, so downstream code is unchanged.
- CLI `load_scene` uses the compact reader for all commands once parity is
  proven. Keep `BLOCKSCOPE_DENSE_PARSE=1` as a debugging fallback until the
  module is complete.

### 4. Policy

Use sparse when:

```text
sparse_bytes = occupied * entry_size + header
dense_bytes  = volume * bits / 8
sparse_bytes * 2 < dense_bytes
```

and the region volume is above a small threshold, so tiny regions do not pay
the sparse lookup cost. Tune the constants from `memory-baseline` measurements;
do not guess them in code comments.

## Implementation Slices

### S0: Spike and go/no-go

Build a standalone prototype that reads one `BlockStates` array in bounded
chunks and counts/decodes occupied values. Measure peak and wall time against
the dense parser on the sparse fixtures. If the peak win is below 50 percent or
the parse time more than doubles on the target files, stop and record the
result in `docs/performance.md`.

### S1: `RegionStates` wrapper

Add the enum with only `Dense`. Migrate `Region`, `Scene`, mesh, LOD, statistics,
manifest, and tests. No behavior change; all hashes and goldens must be
identical.

### S2: `SparseIndices`

Implement construction from dense values, `get`, `iter_occupied`, `counts`, and
canonical hashing. Add property tests against `PackedIndices` for random
palettes and values.

### S3: Compact parser

Implement the two-pass reader and `LitematicFile::parse_bytes_compact`. Compare
its `Scene` against the dense parser on every fixture: bounds, statistics,
diagnostics, block entities, scene hash, and manifest JSON.

### S4: CLI wiring

Use the compact path in `load_scene`, keep the env fallback, and measure the
existing real fixtures with `bench`. Verify `--stream-mesh`, LOD, multi-view,
and the stdio service still produce identical results.

### S5: Budgets and docs

Tighten the `memory-baseline` sparse budget, record before/after numbers, and
document the representation and fallback.

## Testing Strategy

- **Unit:** `SparseIndices` get/iter/counts/hash parity with `PackedIndices` over
  random packed arrays, including values crossing 64-bit word boundaries.
- **Property:** sparse and dense states produce the same canonical word stream
  and the same scene hash for the same values.
- **Integration:** every existing litematic fixture parses through both paths;
  compare `Scene` fields, manifest JSON, and rendered PNG SHA-256 at LOD 1.
- **Memory:** `memory-baseline` sparse scaling assertion passes; dense peak
  regression stays within 5 percent.
- **Performance:** record parse wall time and peak for `000001`, `000002`, and a
  synthetic `1024^3` sparse file. Two-pass decompression is acceptable only if
  the compact path is selected above a measured threshold.
- **Errors:** truncated gzip, trailing bytes, short/long `BlockStates`, invalid
  palette index, skipped `TileEntities`, and limit failures return the same
  error code and message as the dense path.

## Boundaries

- **Always:** preserve scene hash, manifest schema, PNG pixels, diagnostics, and
  error codes; run the dense parser as an oracle; measure before keeping an
  optimization.
- **Ask first:** changing `Region.states`' public type; raising a resource limit;
  making the compact path the default before parity is proven; adding a new
  dependency.
- **Never:** copy code from the AGPL reference implementation; commit private
  `.litematic` files or resource packs; delete or skip golden tests to make the
  new path pass.

## Success Criteria

- A sparse `512^3` region with three blocks has a `parse+scene` peak below the
  budget set by `memory-baseline` (target: at least 80 percent below the dense
  baseline), and a `1024^3` sparse fixture stays within a small constant
  multiple of the `512^3` peak.
- Dense `256^3` peak regression is at most 5 percent; dense parse wall time
  regression is at most 10 percent.
- Scene hash, manifest JSON, diagnostics, and LOD 1 PNG SHA-256 are identical to
  the dense parser for all committed fixtures.
- The full workspace gate and the focused memory oracle pass.
- `docs/performance.md` contains the before/after table and the exact commands.

## Open Questions

1. Is changing `Region.states` from `PackedIndices` to `RegionStates` acceptable
   for this 0.x workspace, or should a parallel `CompactScene` type keep `Scene`
   untouched? Proposed: change `Region.states`; it is the smaller long-term
   surface.
2. Which target matters more: sparse bounding-box memory or dense exact-render
   peak? Proposed: sparse only; dense exact stays with `--stream-mesh` and LOD.
3. Is a two-pass gzip read (roughly 2x decompression CPU) acceptable for large
   sparse files if the compact path is only used above a size threshold?
   Proposed: yes, with a measured threshold and a dense fallback.
4. Must the scene hash preserve non-zero padding bits in the final
   `BlockStates` word? Proposed: preserve them; if that requires storing one
   extra word per region, do it.
5. Should `inspect`, `validate`, and `stats` use the compact parser, or only
   `render`, `mesh`, and `bench`? Proposed: all paths after parity, because
   otherwise their peak memory still scales with volume.
