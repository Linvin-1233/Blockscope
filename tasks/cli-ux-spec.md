# Spec: CLI Presentation Layer

**Status:** Implemented 2026-10-28 (static banner tiers and live progress resize verified)
**Date:** 2026-10-28
**Owner:** CLI

## Objective

Make the `blockscope` command line feel deliberate and easy to scan for humans
without changing anything that scripts depend on. The current CLI prints plain
`key: value` lines with no color, no alignment, no progress feedback, and a
bare `blockscope` invocation is a clap usage error.

Success means a first-time user can run `blockscope`, understand what the tool
does and where to start, and can watch a long render make progress instead of
wondering whether it hung. Existing JSON output, exit codes, and text
substrings used by tests and integrations must keep working.

Scope covers:

1. A global color mode and a small shared style layer.
2. A decorated no-argument welcome screen with a text logo.
3. A complete option list in `--help` with explicit value names and clearer
   error/warning rendering.
4. Consistent, lightly styled human-readable result output.
5. TTY-only progress feedback for long commands.

Not in scope: a full-screen interactive TUI, changes to `serve` /
`supervise` protocols, any change to JSON schemas, or rendering behavior.

## Assumptions

1. "TUI" in the request means a welcoming landing screen when `blockscope`
   runs without a subcommand, not an interactive full-screen application.
   A real TUI would be a separate spec.
2. The logo is the 6-line ANSI Shadow wordmark supplied by the user. It steps
   down by terminal width: 82 columns wide, 59 columns compact, 47 columns
   small, then a plain text title. Redirected output uses the compact mark.
   No layout line ever exceeds the detected width.
3. Colors are enabled only on a terminal, or when the user passes
   `--color always`. `NO_COLOR` disables color for `--color auto`.
4. Progress is written to stderr only, only when stderr is a terminal, and
   never for `--json` runs. Piped output stays byte-identical to today.
5. Existing human output keeps its current `key: value` lines and substrings.
   The polish is color, section spacing, and table alignment, not a rewrite.
6. Adding `anstream`, `anstyle`, and `indicatif` is approved (the user allowed
   mature dependencies; the first two are already in the dependency graph via
   clap).
7. An interactive TUI is out of scope. The static banner adapts to the width
   and height detected at print time; after the process exits, the terminal
   owns the characters and may reflow them on resize. Only live progress
   widgets are reflowed by the program, because the command is still running.
8. Interactive progress widgets reflow on terminal resize: a background
   watcher checks the stderr width every 200 ms and updates active spinners or
   bars so their rendered line never exceeds the new width.
8. Progress is measured from real work units, not elapsed-time guesses:
   mesh pages for meshing and rasterizer tiles for full-mesh rendering.
   Streaming rasterization has no stable per-tile denominator because tiles are
   revisited by many page chunks, so it reports page progress plus a spinner
   for final compositing.

Resolved by the user:

- Bare `blockscope` exits `0`.
- Use the ANSI Shadow-style logo shown in the request (85 columns), with the
  narrow-terminal fallback above.
- Static welcome screen now; interactive TUI later.
- Do not stop at stage-level progress: add real percentage progress now.

## Tech Stack

- Rust 2024, `clap` 4 (derive).
- `anstream` 1 + `anstyle` 1 for TTY-aware ANSI output on Windows and Unix.
- `indicatif` 0.17 for spinners and progress bars on stderr.
- Existing test stack: `cargo test`, integration tests that spawn the binary.

## Commands

```bash
# Focused loop
cargo test -p blockscope-cli --test cli
cargo test -p blockscope-cli --test cli_ux
cargo test -p blockscope-cli --test cli_terminal

# Repository gate (see AGENTS.md)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
just check
```

## Project Structure

```text
crates/render/src/rasterizer.rs       Tile progress callback and will_stream()
crates/render/src/rasterizer/tile.rs  Per-tile progress updates in parallel/serial loops
crates/mesh/src/builder.rs            Page progress for the full-mesh builder
crates/mesh/src/builder/stream.rs     Page progress for the streaming builder
crates/cli/src/ui/mod.rs              Color mode resolution, style tokens, out/err writers
crates/cli/src/ui/banner.rs           Logo, version, quick-start examples
crates/cli/src/ui/progress.rs         indicatif wrappers; TTY and --json gating
crates/cli/src/main.rs                Global --color, clap styles, no-arg dispatch, error rendering
crates/cli/src/commands/*.rs          Semantic output + progress stages
crates/cli/tests/cli_ux.rs            New behavior tests (banner, color, JSON purity, help)
docs/zh/user/cli.md                 Chinese CLI docs
docs/user/cli.md                    English CLI docs
CHANGELOG.md / CHANGELOG.en.md        User-visible changes
```

`ui` stays under 800 lines per file and is split by concern. Internal helpers
stay `pub(crate)`; no library API changes.

## Code Style

```rust
// Style tokens are const; output goes through the TTY-aware writer.
let stage = progress.stage("Building mesh");
let mesh = build_scene_mesh(...)?;
stage.finish();

ui::outln!("{} {}", ui::label("output:"), ui::accent(path.display()));
ui::outln!("{}", ui::success("OK"));
```

- Identifiers English, comments English; Chinese only for user-facing docs.
- No emoji. ASCII logo, ASCII progress spinner (`- \ | /`).
- Color never encodes information alone: `OK`/`WARNING`/`ERROR` text remains.
- JSON paths call `println!` directly and never pass through the style layer.

## Behavior Contract

### Color

- Global flag: `--color <auto|always|never>`, valid before or after a
  subcommand because it is a clap `global` argument.
- `auto` (default): color only when the stream is a terminal and `NO_COLOR`
  is not set. `CLICOLOR_FORCE` may force color, matching `anstream`.
- `always`: emit ANSI even through a pipe.
- `never`: emit no ANSI.
- `--color` also controls clap help/error coloring and our own output.
- JSON payloads are never styled, even with `--color always`.
- `TERM=dumb` behaves like no color under `auto`.

### Welcome screen (`blockscope` with no subcommand)

- Exit code `0` instead of clap's "subcommand required" error.
- Prints the logo, `Blockscope 0.4.1`, a one-line description, quick-start
  commands, and a pointer to `blockscope help`.
- Adapts to the detected terminal width at print time: wide logo and full body
  at 84+ columns, compact logo and short body at 62+, small logo and short
  body at 50+, plain title and narrow body at 30+, and a minimal two-line hint
  below that. No line is longer than the terminal width.
- `blockscope --help` keeps the normal clap help (banner is only for the
  no-argument case), with `wrap_help` enabled so help wraps to the terminal.
- Works through a pipe with color stripped and the compact mark.
- The program cannot redraw a static banner after it exits; terminal reflow
  after that point is outside the CLI's control.

### Help and errors

- Top-level help gets an examples block and a note about `--color`.
- `render`, `bench`, `mesh`, and `serve` list every option under `Options`;
  there are no separate option headings that could hide part of the list.
- Option value names are explicit where the value is not obvious
  (`--color <WHEN>`, `--background <COLOR>`).
- Clap errors keep their existing text and exit code `2`; the error prefix is
  styled red.
- Runtime errors print `error: <message>` on stderr, styled red only on TTY.
  The `CODE:` text inside the message is unchanged.
- The `--force-limits` warning becomes `warning: <message>` (yellow on TTY).

### Human-readable result output

- `inspect` / `validate`: labels dimmed, paths and `OK` accented, diagnostic
  severities colored (`error` red, `warning` yellow, `info` dim).
- `stats` / `materials`: aligned columns with a dim header row; counts right
  aligned; `minecraft:*` state names untouched.
- `mesh` / `render` / `bench` / `capacity`: existing lines kept, with accent
  on output paths and dim on timings and hashes. Multi-view render output gets
  a `view 1/2` heading (the per-view lines stay the same).
- No key padding is introduced on `key: value` lines, so current tests and
  scripts that match `volume: 2 (air 1, non-air 1)` keep working.
- `--json` output remains exactly the schema it is today.

### Progress

- Long commands (`render`, `bench`, `mesh`, `materials`) show progress on
  stderr: parse, LOD, materials, mesh, render, PNG write.
- Meshing reports `completed_pages / total_pages` from the mesh builder.
  The full-mesh and streaming builders both expose page totals.
- Full-mesh rendering reports `completed_tiles / total_tiles` from the
  rasterizer, one update per finished tile.
- Multi-view rendering uses a `view n/total` bar because views render in
  parallel; tile-level progress is used for single-view renders only.
- Streaming rasterization shows page progress plus an indeterminate
  compositing spinner, because a tile is revisited by many pages and a
  per-tile percentage would move backwards.
- Spinners use ASCII ticks and `{msg} ({elapsed})`; work bars use
  `{msg} [{bar}] {pos}/{len}`. On narrow stderr, the bar shortens to 16 cells,
  then to `{msg} {pos}/{len}`, then to a bare counter, and long messages are
  truncated so a redraw never wraps.
- A background width watcher reflows every active widget within 200 ms of a
  terminal resize, including spinners whose text was already truncated.
- Enabled only when stderr is a terminal, `--json` is off, `NO_COLOR` is not
  set, and color is not `never`.
- Cleared before the final summary and before any error line, via `Drop`.
- No progress on `inspect`, `validate`, `stats`, `serve`, or `supervise`.

## Testing Strategy

New `crates/cli/tests/cli_ux.rs` integration tests, using the existing
`support::run` helper and fixtures:

| Test | Assertion |
|---|---|
| `no_args_prints_welcome_and_exits_zero` | status success, stdout has `Blockscope`, version, `render` example |
| `welcome_is_plain_when_piped` | no `\x1b[` in captured stdout |
| `color_always_emits_ansi` | `--color always validate` contains `\x1b[` and `OK:` |
| `color_never_emits_no_ansi` | `--color never validate` has no `\x1b[` |
| `json_is_never_styled` | `--color always inspect --json` parses as JSON, no `\x1b[` |
| `help_lists_render_options` | `render --help` lists all key flags under `Options` |
| `help_mentions_color` | top-level `--help` contains `--color` |
| `render_piped_has_no_progress` | successful piped render leaves stderr empty |
| `error_prefix_is_plain_when_piped` | invalid file stderr starts with `error:` and keeps the code |

Unit tests inside `ui` cover color resolution precedence and banner content.

Existing `cli.rs` and `cli_terminal.rs` tests must pass unchanged. A manual
check on Windows Terminal verifies ANSI rendering, logo alignment at 80 and
120 columns, and spinner behavior during a real render.

## Boundaries

- **Always:** keep JSON pure; keep exit codes `0/1/2`; run `just check`
  before declaring done; keep output readable with color disabled.
- **Ask first:** adding dependencies beyond those listed; changing command
  semantics beyond the no-argument banner; adding an interactive TUI;
  changing `serve`/`supervise` output.
- **Never:** print styled or progress output to stdout before JSON; emit
  progress when the stream is not a TTY; add emoji; require Unicode for the
  logo; break the existing `key: value` substrings.

## Success Criteria

1. Running `blockscope` prints the logo, version, and quick-start examples,
   and exits `0` immediately. The layout fits the detected terminal width.
2. `blockscope --color never validate file.litematic` contains no ANSI bytes;
   `--color always` does contain them.
3. `blockscope --color always inspect file.litematic --json` is valid JSON
   with no ANSI bytes.
4. `blockscope render --help` lists every option under `Options` and shows examples.
5. A piped `render` invocation writes no progress to stdout or stderr and
   keeps the current human summary lines.
6. An interactive `render` on a terminal shows page/tile percentage progress
   on stderr and clears it before the summary; the final position equals the
   reported total for meshing and for single-view full-mesh rasterization.
   Resizing the window mid-run reflows the active widget without wrapping.
7. All existing CLI tests pass without edits to their assertions (except
   where a changed line is explicitly expected and reviewed).
8. `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
   warnings`, and `cargo test --workspace` pass.
9. `docs/zh/user/cli.md`, `docs/user/cli.md`, and both changelogs describe
   `--color`, the welcome screen, and progress behavior.

## Open Questions

Resolved:

1. Welcome screen exit code is `0`.
2. Logo is the 6-line ANSI Shadow wordmark with a narrow-terminal fallback.
3. A static welcome screen is enough for now; interactive TUI is deferred.
4. Real percentage progress is in scope for meshing and full-mesh rendering;
   streaming compositing stays indeterminate by design.
