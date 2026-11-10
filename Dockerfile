# Multi-stage Docker build for FMD3 San (三) Headless Daemon
FROM rust:trixie AS builder

WORKDIR /usr/src/fmd3
COPY . .

RUN cargo build --release --bin fmd

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /usr/src/fmd3/target/release/fmd /usr/local/bin/fmd
COPY --from=builder /usr/src/fmd3/lua /app/lua

ENV RUST_LOG=info
EXPOSE 8088

VOLUME ["/app/data", "/app/downloads"]

ENTRYPOINT ["fmd"]
CMD ["serve", "--bind", "0.0.0.0:8088"]
