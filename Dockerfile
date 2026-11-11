# syntax=docker/dockerfile:1
# Multi-stage Docker build for FMD3 San (三) Headless Daemon
# Cross-compiles natively on the host architecture (BUILDPLATFORM) without slow QEMU emulation
FROM --platform=$BUILDPLATFORM rust:trixie AS builder

ARG TARGETARCH

# Install cross-compilation toolchain for arm64 if targeted
RUN if [ "$TARGETARCH" = "arm64" ]; then \
        apt-get update && apt-get install -y --no-install-recommends \
            gcc-aarch64-linux-gnu \
            libc6-dev-arm64-cross \
        && rm -rf /var/lib/apt/lists/* \
        && rustup target add aarch64-unknown-linux-gnu; \
    else \
        rustup target add x86_64-unknown-linux-gnu; \
    fi

WORKDIR /usr/src/fmd3
COPY . .

# Build with BuildKit cache mounts isolated per architecture to prevent parallel race conditions
RUN --mount=type=cache,id=cargo-registry-${TARGETARCH},target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git-${TARGETARCH},target=/usr/local/cargo/git \
    --mount=type=cache,id=cargo-target-${TARGETARCH},target=/usr/src/fmd3/target \
    if [ "$TARGETARCH" = "arm64" ]; then \
        export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
               CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc \
               RUST_TARGET="aarch64-unknown-linux-gnu"; \
    else \
        RUST_TARGET="x86_64-unknown-linux-gnu"; \
    fi && \
    cargo build --release --target "$RUST_TARGET" --bin fmd && \
    mkdir -p /out && \
    cp "/usr/src/fmd3/target/$RUST_TARGET/release/fmd" /out/fmd

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /out/fmd /usr/local/bin/fmd
COPY --from=builder /usr/src/fmd3/lua /app/lua

ENV RUST_LOG=info
EXPOSE 8088

VOLUME ["/app/data", "/app/downloads"]

ENTRYPOINT ["fmd"]
CMD ["serve", "--bind", "0.0.0.0:8088"]
