# syntax=docker/dockerfile:1

FROM rust:bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY xtask ./xtask

# GPU support needs host device passthrough and drivers at run time. The
# default image stays CPU-only; build with --build-arg GPU=1 for a GPU-enabled
# image on a platform that can load a driver.
ARG GPU=0
RUN if [ "$GPU" = "1" ]; then \
        cargo build --release -p blockscope-cli --features gpu; \
    else \
        cargo build --release -p blockscope-cli; \
    fi

FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="Blockscope"
LABEL org.opencontainers.image.description="Headless Litematic image rendering engine"

RUN useradd --create-home --uid 10001 render
COPY --from=builder /src/target/release/blockscope /usr/local/bin/blockscope

USER render
WORKDIR /data

ENTRYPOINT ["blockscope"]
CMD ["serve"]
