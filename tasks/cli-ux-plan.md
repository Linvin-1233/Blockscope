# Plan: CLI Presentation Layer

**Status:** Implemented; static banner tiers and live progress resize verified
**Date:** 2026-10-28
**Spec:** [cli-ux-spec.md](cli-ux-spec.md)

## Slices

### Slice 1: UI foundation, color, help, welcome

- Add `anstream`, `anstyle`, `console`, `indicatif` to the workspace.
- Add `crates/cli/src/ui/mod.rs`: color mode, TTY-aware writers, style tokens.
- Add `crates/cli/src/ui/banner.rs`: ANSI Shadow logo, compact fallback,
  version, quick-start block.
- Add `--color auto|always|never` as a global clap argument; apply it to clap
  help and errors through an early argv scan.
- Make the subcommand optional; no arguments prints the banner and exits 0.
- Add clap styles, a complete `Options` list, and examples.
- Style runtime errors as `error:` and forced-limit warnings as `warning:`.
- Verify: new `cli_ux` tests plus existing CLI tests.

### Slice 2: Human-readable output

- `inspect` / `validate`: dim labels, accent paths, color `OK`, color
  diagnostic severities.
- `stats` / `materials`: aligned tables with dim headers; keep state names.
- `mesh` / `render` / `bench` / `capacity`: accent output paths and headings,
  dim timings/hashes; keep every existing `key: value` substring.
- Verify: existing assertions pass unchanged; add table/heading checks.

### Slice 3: Real progress

- Mesh crate: page progress callbacks for the parallel and streaming builders.
- Render crate: tile progress callbacks for the full-mesh rasterizer plus
  `will_stream()`.
- CLI: `ui/progress.rs` wrappers gated by TTY, `--json`, and `--color never`.
- Wire parse, materials, mesh pages, render tiles, and PNG write stages into
  `render`, `bench`, `mesh`, and `materials`.
- Multi-view uses a view counter; streaming compositing uses a spinner.
- Verify: callback unit/integration tests, piped stderr stays empty.

### Slice 4: Docs and gate

- Update `docs/user/cli.md`, `docs/user/en/cli.md`, both changelogs, and
  `tasks/todo.md`.
- Run `cargo fmt --all --check`, clippy, the full test suite, and a manual
  Windows Terminal check.

### Slice 5: Static banner width tiers

- Refactor the banner into reusable line builders and add the 82/59/47/plain
  title tiers, fitting the detected terminal width.
- No alternate screen or blocking welcome screen.

### Slice 6: Live progress resize

- Add a global width watcher and a weak registry of active progress widgets.
- Recompute template and message width on resize, force a redraw, and keep
  every line within the new width.
- Verify with a PTY: start a render at 80 columns, resize to 40 mid-render,
  and assert no captured progress line exceeds 40 visible columns.

## Verification

- `cargo fmt --all --check`: pass
- `cargo clippy --workspace --all-targets -- -D warnings`: pass
- `cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings`: pass
- `cargo test --workspace`: pass, including new `cli_ux`, mesh progress, and
  render progress tests
- PTY check with pywinpty at 120, 80, 55, 40, 28, and 18 columns: the static
  banner selects the expected tier and every visible line fits the width
- PTY render at 80 columns, resized to 40 mid-run: every progress line after
  the resize stays within 40 visible columns; the render still completes

## Risks

- `indicatif` redraw on Windows: use `ProgressBar` on stderr only, verify in
  Windows Terminal.
- Callback overhead: one atomic increment per tile/page, no per-pixel work.
- JSON purity: none of the JSON paths go through the UI writer.
- Existing user changes in the working tree: touch only CLI presentation and
  the progress callback plumbing; do not commit unrelated files.
