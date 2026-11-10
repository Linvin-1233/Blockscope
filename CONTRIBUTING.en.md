# Contributing

[中文](CONTRIBUTING.md)

Thanks for improving Blockscope. This guide covers the development
environment, code conventions, tests, asset policy, and pull-request workflow.

Read these first:

- [README.en.md](README.en.md)
- [CODE_OF_CONDUCT.en.md](CODE_OF_CONDUCT.en.md)
- [CHANGELOG.en.md](CHANGELOG.en.md)

## What to Contribute

- Fix parsing, rendering, material, CLI, protocol, or documentation issues
- Add tests, benchmarks, and minimal reproducible fixtures
- Improve errors, diagnostics, and logs
- Optimize large structures, memory, and rendering performance
- Expand format, protocol, and deployment documentation

For a large feature or interface change, open an issue first to align on the
goal and scope.

## Development Environment

Rust stable is required. The project uses edition 2024; Rust 1.85 or newer is
recommended.

```bash
git clone https://github.com/Linvin-1233/Blockscope.git
cd Blockscope
cargo build --release -p blockscope-cli
```

Windows, Linux, and macOS are supported. Real resource packs, client jars, and
`.litematic` samples stay local and are never committed.

## Quality Gates

Before submitting:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Or run `just` directly:

```bash
just check
# or without just
cargo xtask check
```

`just check` matches the CI test matrix, which runs the same checks on Ubuntu,
Windows, and macOS. Use `just check-all` for the extra gates: `check-linux` runs
clippy against the `x86_64-unknown-linux-gnu` target to lint Linux-only code
(the first run installs that target and needs network access), and `check-wasm`
checks the Worker's wasm target.

Repository automation lives in the `xtask` crate and runs as
`cargo xtask <command>`. New tasks belong there, not in separate scripts.

The CI test matrix runs the same checks on Ubuntu, Windows, and macOS; the
container and MSI jobs run on Ubuntu and Windows respectively. Do not skip tests,
remove assertions, add `#[ignore]`, or weaken lints just to get a green run. If
a rule really needs to change, explain why in the pull request.

## Repository Layout

| Path               | Responsibility                                                |
| ------------------ | ------------------------------------------------------------- |
| `crates/nbt`       | NBT decoding, compression detection, limits, error codes      |
| `crates/litematic` | `.litematic` container, region, palette, block entity parsing |
| `crates/scene`     | Scene IR, statistics, diagnostics, manifest, hash             |
| `crates/material`  | Blockstates/models, resource packs, textures, tint, chest     |
| `crates/mesh`      | Color-block/pack meshes, culling, AO, OBJ/PLY export          |
| `crates/render`    | CPU/GPU rasterization, tiles, supersampling, PNG              |
| `crates/video`     | Keyframed camera timeline, ffmpeg MP4/GIF encoding            |
| `crates/cli`       | `blockscope` commands, serve/supervise, animate, capacity     |
| `crates/worker`    | Cloudflare Workers HTTP adapter                               |
| `xtask`            | Repository automation (checks, builds, releases)              |
| `crates/*/tests`   | Integration, golden, and real-file tests                      |

Dependencies should flow in one direction: data crates do not depend on the CLI,
and the render crate does not depend on the CLI. Public protocols and error codes
are versioned. Do not leak internal buffers, temporary files, or parser internals
into protocol DTOs.

## Code Conventions

- Use `rustfmt` with `max_width = 100` and 4-space indentation
- `snake_case` for functions and variables, `PascalCase` for types and traits
- Use `//` comments that explain intent, not restate code
- Keep Rust source files under 800 lines; split modules when larger
- Reuse existing helpers and crates; justify any new dependency
- Return stable error codes, not only strings
- Update `docs/engine-protocol.md` and `MANIFEST_SCHEMA_VERSION` or
  `PROTOCOL_VERSION` for public protocol changes
- Update `docs/user` and `docs/zh/user`, `CHANGELOG.en.md`, help text, and tests for CLI
  changes
- Do not commit `target/`, `renders/`, `cache/`, logs, or IDE configuration

`AGENTS.md` contains additional conventions for coding agents. Follow it when
using AI tooling.

## Testing

### Regular Tests

```bash
cargo test --workspace
```

Run one crate:

```bash
cargo test -p blockscope-nbt
cargo test -p blockscope-scene
cargo test -p blockscope-core
cargo test -p blockscope-material
cargo test -p blockscope-mesh
cargo test -p blockscope-render
cargo test -p blockscope-cli
```

New behavior needs tests; bug fixes should include a regression test written
before the fix. Test behavior, not internal implementation details.

### Real-Fixture Tests

Real-file tests are ignored by default and need local environment variables:

```bash
# Real .litematic parsing
LITEMATIC_FIXTURES_DIR=/path/to/fixtures \
  cargo test -p blockscope-core --test real_files -- --ignored

# Real Minecraft client jar
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p blockscope-material --test real_jar -- --ignored

# Real material rendering
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p blockscope-render --test real_materials -- --ignored
```

Windows uses different environment-variable syntax; pass the path as an
argument instead (`just test-real-litematic D:\path\to\fixtures`):

```bash
# The command works the same on Windows, Linux, and macOS
cargo xtask test-real litematic --path /path/to/fixtures
cargo xtask test-real jar --path /path/to/client.jar
```

Do not commit real fixtures, client jars, or rendered artifacts.

### Golden Files

Golden manifests and PNGs catch deterministic regressions. Only update them when
a rendering/format change is intentional:

```bash
# Linux/macOS
UPDATE_GOLDEN=1 cargo test -p blockscope-cli --test golden
UPDATE_GOLDEN=1 cargo test -p blockscope-render --test golden_png

# Or xtask, on any platform
cargo xtask golden-update
```

Review the `fixtures/golden/` diff and confirm that every change is expected.
Do not regenerate goldens without an explanation.

## Commits and Pull Requests

- One logical change per commit; keep refactors, features, and fixes separate
- Use existing prefixes: `feat:`, `fix:`, `perf:`, `refactor:`, `test:`,
  `docs:`, `chore:`
- Explain what changed and why; avoid messages like `fix bug`
- Update `docs/user` and `docs/zh/user`, `CHANGELOG.en.md`, protocol docs, and tests with
  behavior changes
- The PR description should cover context, changes, verification commands, and
  known limitations
- Keep `docs/user`, `docs/zh/user`, and the root README navigation in sync with user-visible
  behavior changes
- Include before/after output or golden diffs for rendering changes
- Keep PRs reviewable; split large changes into smaller PRs

Before submitting:

```bash
git status --short
git diff --check
```

## Assets and Privacy

Do not commit:

- Minecraft client jars, resource packs, textures, or other Minecraft assets
- Private `.litematic` files, server worlds, or personal world data
- Real user uploads, absolute paths in logs, or identity information
- Code or assets copied from an unlicensed reference implementation

Use synthetic fixtures in `tests/support/` or load real files locally through
environment variables. Remove file names, server addresses, and other sensitive
data before opening an issue.

## Reporting Issues

Include:

- OS, Rust version, and `blockscope --version`
- The full command or protocol request, with sensitive paths removed
- Actual and expected behavior
- Error code and relevant stderr
- A minimized, shareable synthetic fixture when possible

For security vulnerabilities, path traversal, denial of service, or data leaks,
do not open a public issue first. Contact the maintainer through the GitHub
profile and include impact, reproduction steps, and a suggested fix.

## Protocol and Compatibility

- `docs/engine-protocol.md` is the serve/supervise contract
- Manifest changes require evaluating `MANIFEST_SCHEMA_VERSION`
- Error codes are a stable API; do not rename them casually
- Prefer backward-compatible protocol additions; bump `PROTOCOL_VERSION` for
  removals or semantic changes
- Update `docs/user` and `docs/zh/user`, help text, and tests when CLI defaults or
  limits change

## Code of Conduct

By participating, you agree to follow
[CODE_OF_CONDUCT.en.md](CODE_OF_CONDUCT.en.md). Violations may result in content
removal or temporary/permanent exclusion from the project spaces.
