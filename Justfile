# Blockscope developer workflow recipes
# Run `just` or `just --list` to view available recipes.

# Display available recipes and descriptions
default:
    @just --list

# Format all workspace code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all --check

# Run Clippy lints across all workspace targets with warnings denied
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Check the Worker crate for the wasm runtime target
check-wasm:
    cargo clippy -p blockscope-worker --target wasm32-unknown-unknown -- -D warnings

# Apply Clippy suggestions automatically where possible
clippy-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty --allow-staged

# Cross-target Clippy for Linux to catch platform-specific code (e.g. cgroups, RSS)
[unix]
check-linux:
    @if command -v rustup >/dev/null 2>&1; then \
        if ! rustup target list --installed | grep -q "^x86_64-unknown-linux-gnu$"; then \
            echo "Installing x86_64-unknown-linux-gnu target..."; \
            rustup target add x86_64-unknown-linux-gnu; \
        fi; \
    fi
    cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings

[windows]
check-linux:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-linux.ps1

# Run tests across all workspace crates
test *args:
    cargo test --workspace {{ args }}

# Run tests for a specific package (e.g. `just test-pkg blockscope-cli`)
test-pkg pkg *args:
    cargo test -p {{ pkg }} {{ args }}

# Run ignored tests (e.g. real file tests)
test-ignored *args:
    cargo test --workspace -- --ignored {{ args }}

# Run real .litematic fixture tests (requires LITEMATIC_FIXTURES_DIR env var or directory argument)
[unix]
test-real-litematic dir=env_var_or_default("LITEMATIC_FIXTURES_DIR", "") *args:
    LITEMATIC_FIXTURES_DIR="{{ dir }}" cargo test -p litematic-core --test real_files -- --ignored {{ args }}

[windows]
test-real-litematic dir=env_var_or_default("LITEMATIC_FIXTURES_DIR", "") *args:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-real.ps1 -Type litematic -Path "{{ dir }}" -Args "{{ args }}"

# Run real client JAR texture extraction tests (requires MINECRAFT_CLIENT_JAR env var or jar path argument)
[unix]
test-real-jar jar=env_var_or_default("MINECRAFT_CLIENT_JAR", "") *args:
    MINECRAFT_CLIENT_JAR="{{ jar }}" cargo test -p litematic-material --test real_jar -- --ignored {{ args }}
    MINECRAFT_CLIENT_JAR="{{ jar }}" cargo test -p litematic-render --test real_materials -- --ignored {{ args }}

[windows]
test-real-jar jar=env_var_or_default("MINECRAFT_CLIENT_JAR", "") *args:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-real.ps1 -Type jar -Path "{{ jar }}" -Args "{{ args }}"

# Run standard quality gate: format check, clippy, and tests (matches scripts/check.sh)
check: fmt-check clippy test

# Build the Cloudflare Workers bundle into .cloudflare/output/v0
[unix]
worker:
    ./scripts/build-worker.sh

[windows]
worker:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-worker.ps1

# Deploy the prebuilt Cloudflare Worker
[unix]
worker-deploy: worker
    cf deploy --prebuilt

[windows]
worker-deploy: worker
    cf deploy --prebuilt

# Run all quality gates including the cross-target Linux and wasm checks
check-all: check check-linux check-wasm

# Build all workspace crates in debug mode
build:
    cargo build --workspace

# Build optimized release binary of the Blockscope CLI
build-release:
    cargo build --release --locked -p blockscope-cli --bin blockscope

# Run the Blockscope CLI with arbitrary arguments
run *args:
    cargo run -p blockscope-cli -- {{ args }}

# Run the Blockscope service daemon (stdio JSON protocol)
serve *args:
    cargo run -p blockscope-cli -- serve {{ args }}

# Inspect machine capacity and recommended pool limits
capacity *args:
    cargo run -p blockscope-cli -- capacity {{ args }}

# Run CLI performance benchmarks
bench *args:
    cargo run --release -p blockscope-cli -- bench {{ args }}

# Run the render tile benchmark example
bench-tile *args:
    cargo run --release -p litematic-render --example tile_bench {{ args }}

# Regenerate golden test manifests and reference PNGs
[unix]
golden-update:
    UPDATE_GOLDEN=1 cargo test -p blockscope-cli --test golden
    UPDATE_GOLDEN=1 cargo test -p litematic-render --test golden_png

[windows]
golden-update:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/golden-update.ps1

# Build local Docker container image
docker-build tag="blockscope:local":
    docker build -t {{ tag }} .

# Run capacity and protocol smoke tests inside Docker container (matches CI)
[unix]
docker-smoke tag="blockscope:local":
    docker run --rm --cpus=1 --memory=1g {{ tag }} capacity --json
    printf '{"type":"shutdown"}\n' | docker run --rm -i --cpus=1 --memory=1g {{ tag }} serve

[windows]
docker-smoke tag="blockscope:local":
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/docker-smoke.ps1 -Tag "{{ tag }}"

# Build release CLI binary with static CRT (Windows: +crt-static)
[unix]
build-static:
    RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --locked -p blockscope-cli --bin blockscope

# Build release CLI binary with static CRT (Windows: +crt-static)
[windows]
build-static:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-static.ps1

# Build Windows MSI installer package (Windows only)
[windows]
build-msi out="out/blockscope-msi-check.msi" bin_dir="target/release" version="":
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-msi.ps1 -Output "{{ out }}" -BinDir "{{ bin_dir }}" -Version "{{ version }}"

# Test MSI installer lifecycle: install, verify, uninstall, cleanup (Windows only)
[windows]
test-msi msi="out/blockscope-msi-check.msi" log_dir="out":
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-msi.ps1 -Msi "{{ msi }}" -LogDir "{{ log_dir }}"

# Verify that a git tag matches the version in Cargo.toml
[unix]
verify-tag tag:
    @tag="{{ tag }}"; \
    tag="${tag#v}"; \
    ver=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1); \
    if [ -z "$ver" ]; then echo "Cannot read version from Cargo.toml"; exit 1; fi; \
    if [ "$tag" != "$ver" ]; then echo "Tag {{ tag }} does not match workspace version $ver"; exit 1; fi; \
    echo "Tag {{ tag }} matches workspace version $ver"

[windows]
verify-tag tag:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/verify-tag.ps1 -Tag "{{ tag }}"

# Package and upload MSI release asset to GitHub Release (Windows only)
[windows]
release-msi tag:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-msi.ps1 -Tag "{{ tag }}"

# A bump raises the workspace version, refreshes Cargo.lock, updates every
# document that states the current version, and promotes the `[Unreleased]`
# changelog notes to the new version, because the release workflow takes the
# release body from that section and fails without it.
#
# The recipe runs only on main from a clean tree, so the tag names exactly the
# reviewed code. Push the commit and the tag yourself; the tag starts the
# release workflow.

# Cut a release: raise the workspace version, commit, and tag it.
[script("bash")]
[unix]
bump level="patch":
    set -euo pipefail

    level="{{ level }}"
    case "$level" in
      major|minor|patch) ;;
      *)
        echo "usage: just bump major|minor|patch" >&2
        exit 1
        ;;
    esac

    branch=$(git branch --show-current)
    if [[ "$branch" != "main" ]]; then
      echo "bump: versions are cut from main, not ${branch:-a detached HEAD}" >&2
      exit 1
    fi

    if [[ -n "$(git status --porcelain)" ]]; then
      echo "bump: workspace has uncommitted changes:" >&2
      git status --short >&2
      exit 1
    fi

    # Cargo.toml is the only version source: every crate inherits
    # version.workspace, and verify-tag, build-msi.ps1, and the release
    # workflow all read this line.
    current=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
    if [[ ! "$current" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
      echo "bump: cannot read the [workspace.package] version from Cargo.toml" >&2
      exit 1
    fi

    next=$(awk -F. -v part="$level" '{
      if (part == "major") printf "%d.0.0\n", $1 + 1
      else if (part == "minor") printf "%d.%d.0\n", $1, $2 + 1
      else printf "%d.%d.%d\n", $1, $2, $3 + 1
    }' <<<"$current")

    if git rev-parse -q --verify "refs/tags/v$next" >/dev/null; then
      echo "bump: tag v$next already exists" >&2
      exit 1
    fi

    # Escape the dots so the replacements match the version literally.
    old_escaped=${current//./\\.}
    new_escaped=${next//./\\.}

    # Every document that states the current version must state it exactly
    # once, so a number missed by an earlier bump fails here instead of
    # shipping stale. The download examples in docs/user/getting-started*.md
    # name the last published release on purpose and are not bumped.
    docs=(README.md README.en.md docs/user/features.md docs/user/en/features.md)
    for doc in "${docs[@]}"; do
      found=$(grep -c -F "$current" "$doc" || true)
      if [[ "$found" != "1" ]]; then
        echo "bump: expected one $current in $doc, found ${found:-0}" >&2
        exit 1
      fi
    done

    today=$(date +%Y-%m-%d)
    for changelog in CHANGELOG.md CHANGELOG.en.md; do
      if ! grep -q '^## \[Unreleased\]$' "$changelog"; then
        echo "bump: no ## [Unreleased] section in $changelog" >&2
        exit 1
      fi
      if grep -q "^## \[$new_escaped\]" "$changelog"; then
        echo "bump: $changelog already has a section for $next" >&2
        exit 1
      fi
      # An empty section would publish a release without notes.
      if ! awk '
        /^## \[Unreleased\]$/ { inside = 1; next }
        inside && /^## \[/ { exit }
        inside && $0 !~ /^[[:space:]]*$/ { noted = 1 }
        END { exit noted ? 0 : 1 }
      ' "$changelog"; then
        echo "bump: $changelog has no unreleased notes for $next" >&2
        exit 1
      fi
    done

    sed -i.bak -E "s/^version = \"$old_escaped\"$/version = \"$next\"/" Cargo.toml
    rm -f Cargo.toml.bak
    for doc in "${docs[@]}"; do
      sed -i.bak "s/$old_escaped/$new_escaped/g" "$doc"
      rm -f "$doc.bak"
    done
    # A fresh [Unreleased] heading goes above the new version heading, so the
    # notes written under [Unreleased] become this version's notes.
    for changelog in CHANGELOG.md CHANGELOG.en.md; do
      buffer=$(mktemp)
      awk -v heading="## [$next] - $today" '
        $0 == "## [Unreleased]" {
          print
          print ""
          print heading
          print ""
          skip_blank = 1
          next
        }
        skip_blank && $0 ~ /^[[:space:]]*$/ { skip_blank = 0; next }
        { skip_blank = 0; print }
      ' "$changelog" >"$buffer"
      cat "$buffer" >"$changelog"
      rm -f "$buffer"
    done

    # Cargo.lock records every workspace member's version. -w rewrites only
    # those entries, and --offline keeps an unchanged dependency set off the
    # network.
    cargo update --workspace --offline

    if ! grep -A1 '^name = "blockscope-cli"$' Cargo.lock | grep -qx "version = \"$next\""; then
      echo "bump: Cargo.lock does not list blockscope-cli $next" >&2
      exit 1
    fi

    # The tree was clean on entry, so only the version and changelog files may
    # differ. Stop before the commit if anything else changed.
    expected=$(printf '%s\n' Cargo.toml Cargo.lock "${docs[@]}" CHANGELOG.md CHANGELOG.en.md | LC_ALL=C sort)
    changed=$(git diff --name-only | LC_ALL=C sort)
    if [[ "$changed" != "$expected" ]]; then
      echo "bump: refusing to commit changes beyond the version bump:" >&2
      git status --short >&2
      exit 1
    fi

    git add Cargo.toml Cargo.lock "${docs[@]}" CHANGELOG.md CHANGELOG.en.md
    git commit -m "chore: bump to v$next"
    git tag -a "v$next" -m "v$next"

    echo "bumped $current to $next, committed and tagged v$next"
    echo "publish with: git push origin main v$next"

# Remove target build artifacts
clean:
    cargo clean
