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
    @powershell -NoProfile -ExecutionPolicy Bypass -Command "\
        $target = 'x86_64-unknown-linux-gnu'; \
        if (Get-Command rustup -ErrorAction SilentlyContinue) { \
            if ((rustup target list --installed) -notcontains $target) { \
                Write-Host 'Installing x86_64-unknown-linux-gnu target...'; \
                rustup target add $target; \
            }; \
        }; \
        cargo clippy --workspace --all-targets --target $target -- -D warnings"

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
    powershell -NoProfile -Command "$env:LITEMATIC_FIXTURES_DIR='{{ dir }}'; cargo test -p litematic-core --test real_files -- --ignored {{ args }}"

# Run real client JAR texture extraction tests (requires MINECRAFT_CLIENT_JAR env var or jar path argument)
[unix]
test-real-jar jar=env_var_or_default("MINECRAFT_CLIENT_JAR", "") *args:
    MINECRAFT_CLIENT_JAR="{{ jar }}" cargo test -p litematic-material --test real_jar -- --ignored {{ args }}
    MINECRAFT_CLIENT_JAR="{{ jar }}" cargo test -p litematic-render --test real_materials -- --ignored {{ args }}

[windows]
test-real-jar jar=env_var_or_default("MINECRAFT_CLIENT_JAR", "") *args:
    powershell -NoProfile -Command "$env:MINECRAFT_CLIENT_JAR='{{ jar }}'; cargo test -p litematic-material --test real_jar -- --ignored {{ args }}; cargo test -p litematic-render --test real_materials -- --ignored {{ args }}"

# Run standard quality gate: format check, clippy, and tests (matches scripts/check.sh)
check: fmt-check clippy test

# Run all quality gates including the cross-target Linux clippy check
check-all: check check-linux

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
    powershell -NoProfile -Command "$env:UPDATE_GOLDEN = '1'; cargo test -p blockscope-cli --test golden; cargo test -p litematic-render --test golden_png"

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
    docker run --rm --cpus=1 --memory=1g {{ tag }} capacity --json
    powershell -NoProfile -Command "'{`"type`":`"shutdown`"}' | docker run --rm -i --cpus=1 --memory=1g {{ tag }} serve"

# Build Windows MSI installer package (Windows only)
[windows]
msi version output="out/blockscope.msi" bin_dir="target/release":
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-msi.ps1 -Version {{ version }} -Output {{ output }} -BinDir {{ bin_dir }}

# Remove target build artifacts
clean:
    cargo clean
