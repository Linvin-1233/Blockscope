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
- Optimize large structures, LOD, memory, and rendering performance
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

Or run the repository script or `just`:

```bash
# Using just
just check

# Linux
bash ./scripts/check.sh

# Windows PowerShell 7+
pwsh ./scripts/check.ps1

# Windows PowerShell 5.1
powershell -NoProfile -ExecutionPolicy Bypass -File ./scripts/check.ps1
```

On Windows, `check.ps1` also runs clippy against the `x86_64-unknown-linux-gnu`
target so Linux-only code is linted locally; the first run installs that target
and needs network access.

The CI test matrix runs the same checks on Ubuntu, Windows, and macOS; the
container and MSI jobs run on Ubuntu and Windows respectively. Do not skip tests,
remove assertions, add `#[ignore]`, or weaken lints just to get a green run. If
a rule really needs to change, explain why in the pull request.

## Repository Layout

| Path               | Responsibility                                                |
| ------------------ | ------------------------------------------------------------- |
| `crates/nbt`       | NBT decoding, compression detection, limits, error codes      |
| `crates/litematic` | `.litematic` container, region, palette, block entity parsing |
| `crates/scene`     | Scene IR, statistics, diagnostics, manifest, hash, LOD        |
| `crates/material`  | Blockstates/models, resource packs, textures, tint, chest     |
| `crates/mesh`      | Color-block/pack meshes, culling, AO, OBJ/PLY export          |
| `crates/render`    | CPU rasterization, tiles, supersampling, PNG                  |
| `crates/cli`       | `blockscope` commands, serve, supervise, capacity detection   |
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
cargo test -p litematic-nbt
cargo test -p litematic-scene
cargo test -p litematic-core
cargo test -p litematic-material
cargo test -p litematic-mesh
cargo test -p litematic-render
cargo test -p blockscope-cli
```

New behavior needs tests; bug fixes should include a regression test written
before the fix. Test behavior, not internal implementation details.

### Real-Fixture Tests

Real-file tests are ignored by default and need local environment variables:

```bash
# Real .litematic parsing
LITEMATIC_FIXTURES_DIR=/path/to/fixtures \
  cargo test -p litematic-core --test real_files -- --ignored

# Real Minecraft client jar
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p litematic-material --test real_jar -- --ignored

# Real material rendering
MINECRAFT_CLIENT_JAR=/path/to/client.jar \
  cargo test -p litematic-render --test real_materials -- --ignored
```

Windows PowerShell:

```powershell
$env:LITEMATIC_FIXTURES_DIR = "D:\path\to\fixtures"
cargo test -p litematic-core --test real_files -- --ignored

$env:MINECRAFT_CLIENT_JAR = "D:\path\to\client.jar"
cargo test -p litematic-material --test real_jar -- --ignored
```

Do not commit real fixtures, client jars, or rendered artifacts.

### Golden Files

Golden manifests and PNGs catch deterministic regressions. Only update them when
a rendering/format change is intentional:

```bash
# Linux/macOS
UPDATE_GOLDEN=1 cargo test -p blockscope-cli --test golden
UPDATE_GOLDEN=1 cargo test -p litematic-render --test golden_png
```

```powershell
# Windows PowerShell
$env:UPDATE_GOLDEN = "1"
cargo test -p blockscope-cli --test golden
cargo test -p litematic-render --test golden_png
Remove-Item Env:UPDATE_GOLDEN
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
