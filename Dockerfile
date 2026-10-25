# syntax=docker/dockerfile:1

FROM rust:bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates

RUN cargo build --release -p blockscope-cli

FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="Blockscope"
LABEL org.opencontainers.image.description="Headless Litematic image rendering engine"

RUN useradd --create-home --uid 10001 render
COPY --from=builder /src/target/release/blockscope /usr/local/bin/blockscope

USER render
WORKDIR /data

ENTRYPOINT ["blockscope"]
CMD ["serve"]
