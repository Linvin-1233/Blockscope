# Spec: `memory-baseline`

**Module id:** `memory-baseline`
**Status:** Draft for review
**Depends on:** none

## Objective

Make peak allocation a reproducible, tested property of litematic parsing and
scene construction. This module establishes the measurement oracle for
`sparse-litematic` and guards it after it lands.

Today the workspace enforces time and output size, but no test asserts how many
bytes `LitematicFile::parse_bytes` and `into_scene` allocate. The missing number
is what let the dense `BlockStates` representation go unnoticed for large,
sparse regions.

Success means a maintainer can run one focused command, see peak bytes for
representative fixtures, and get a red test if parse/scene or streamed render
regresses.

Baseline measured on 2026-10-28 with the current release binary:

| Case | Peak working set |
|---|---|
| `validate 000001` (parse + scene) | 108 MB |
| `validate 000002` (parse + scene) | 132 MB |
| `render 000001 --stream-mesh --force-limits` | 407 MB |
| `bench 000001 --force-limits` (full mesh, color-only) | 6.8 GB |

The full-mesh figure is not a default CI target. The useful guards are parse +
scene and the streamed render path; both are already low enough that a sparse
parser is not the first problem to solve.

## Tech Stack

- Rust 2024 workspace, `litematic-core` integration tests.
- A `GlobalAlloc` wrapper in the test binary with atomic live/peak counters.
- `flate2` for gzip fixture generation. It is already a workspace dependency;
  add it only as a dev-dependency of `litematic-core` if needed.
- No production dependency changes.

## Commands

```bash
# Focused memory oracle
cargo test -p litematic-core --test memory -- --nocapture

# Full gate
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Project Structure

```text
crates/litematic/tests/memory.rs        # single integration test binary
crates/litematic/tests/support/         # streaming NBT fixture helpers
docs/performance.md                     # baseline table, command, and ledger
tasks/todo.md                           # task tracking
```

The test binary owns the global allocator. Keep all measured scenarios in that
one binary so the meter only observes its own process.

## Code Style

- Rust: `rustfmt`, 4 spaces, snake_case functions, PascalCase types, max width
  100.
- Comments explain intent, not mechanics.
- Test names describe the property being guarded, for example
  `sparse_peak_does_not_scale_with_bounding_volume`.
- No emoji.

```rust
fn measure<T>(scenario: impl FnOnce() -> T) -> (T, usize) {
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let output = scenario();
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    (output, peak)
}
```

## Testing Strategy

Use one integration test binary with a `GlobalAlloc` meter. Run measured
scenarios sequentially behind a mutex so parallel test threads cannot pollute a
window.

Fixtures are generated in-process and compressed:

- tiny dense: `2 x 1 x 1`, two palette entries;
- sparse: `256 x 256 x 256` and `512 x 512 x 512` with three stone blocks;
- dense: `128 x 128 x 128` and `256 x 256 x 256` filled with stone;
- multi-region: two overlapping regions with different palettes.

Measure two phases separately:

1. `parse_bytes`: compressed input -> `LitematicFile`.
2. `parse_bytes` + `into_scene`: compressed input -> `Scene`.

Rules:

- Generate compressed fixture bytes before the measured window. Do not build a
  dense `Vec<i64>` for a huge sparse fixture in the test; stream zero words
  through the NBT writer.
- Assert budgets with margin, not exact byte counts. The allocator may round
  capacities.
- Record the sparse scaling gap as a diagnostic. Do not make it a blocking
  assertion unless `sparse-litematic` is approved: default limits reject a
  `512^3` region, so the ratio is not a user-facing regression for normal
  inputs.
- Add a streamed-render budget (about 407 MB for `000001`) in the CLI or render
  test layer. Full-mesh memory is recorded but not gated in default CI; it is
  too large and too close to the host budget.
- Keep the default test runtime under 30 seconds.
- Print every measured number with `--nocapture`, and copy the accepted values
  into `docs/performance.md` with date, commit, OS, and command.

Initial budgets will be set from the first measurement, with a 25 percent
margin. Tightening a budget is allowed only with a new recorded measurement and
review; loosening it to make a failing optimization pass is not.

## Boundaries

- **Always:** run the focused test, the full workspace test, `fmt`, and
  `clippy` before declaring the module done; record real numbers; keep the
  harness independent of platform-specific RSS.
- **Ask first:** adding a production dependency; changing a budget below the
  measured peak; moving a measured scenario to `#[ignore]`; changing CI time
  budget.
- **Never:** commit private `.litematic` files or resource packs; copy code from
  the AGPL reference implementation; weaken an assertion to hide a regression.

## Success Criteria

- `cargo test -p litematic-core --test memory -- --nocapture` passes and prints
  peak bytes for every scenario.
- The recorded peak ratio is visible in the output, and the strict scaling
  assertion is wired for `sparse-litematic` S5. This module itself stays green
  with the baseline budgets.
- `docs/performance.md` records the baseline table and the exact command.
- No production code, CLI behavior, manifest schema, or scene hash changes.
- `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D
  warnings` pass.

## Open Questions

1. Is the CI memory ceiling stable enough for absolute budgets, or should the
   test assert only scaling ratios? Proposed: record absolute numbers, assert
   ratios, and use absolute budgets as generous tripwires.
2. Should the harness cover `LitematicFile::parse_bytes` only, or also the CLI
   `load_scene` path? Proposed: core API first; CLI coverage is redundant until
   the compact parser is wired in.
3. Is a 30-second focused test acceptable in the default workspace test run?
   Proposed: yes for the 256/512 fixtures; keep 1024 fixtures opt-in if they
   exceed the budget.
