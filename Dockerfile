# syntax=docker/dockerfile:1

FROM rust:bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates

RUN cargo build --release -p renderd-cli

FROM debian:bookworm-slim

RUN useradd --create-home --uid 10001 render
COPY --from=builder /src/target/release/renderd /usr/local/bin/renderd

USER render
WORKDIR /data

ENTRYPOINT ["renderd"]
CMD ["serve"]
