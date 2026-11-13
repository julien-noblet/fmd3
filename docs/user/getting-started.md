# Getting Started with FMD2

This guide walks you through setting up and running **Free Manga Downloader 3 — San (FMD3 / FMD三)** on your system or server.

---

## 1. Quick Start with Docker Compose (Recommended)

Docker Compose is the fastest way to run FMD2 on Linux servers, NAS (Synology, TrueNAS, Unraid), or homelabs.

### 1.1 `docker-compose.yml`

Create a `docker-compose.yml` file:

```yaml
services:
  fmd:
    image: ghcr.io/dazedcat19/fmd3:latest
    container_name: fmd3
    restart: unless-stopped
    ports:
      - "8088:8088"
    volumes:
      - ./data:/app/data            # SQLite database and settings
      - ./downloads:/app/downloads  # Destination folder for manga archives
      - ./lua/modules:/app/lua/modules:ro # Custom or community Lua modules
    environment:
      - RUST_LOG=info
      - FMD_PORT=8088
      - FMD_BIND=0.0.0.0
```

### 1.2 Start the Daemon

```bash
docker compose up -d
```

Open your browser and navigate to **http://localhost:8088**.

To serve FMD3 below a URL prefix, set the optional `BASE_PATH` environment variable to a path such as `/reader`. Path-only values with or without leading/trailing slashes are normalized; full URLs, queries, and fragments are rejected. The server generates prefixed UI asset, API, and WebSocket URLs, and accepts both prefixed requests and requests whose ingress has stripped the prefix. If unset or empty, the app is served from `/`.

When a request path begins with the configured prefix on a segment boundary, the server treats it as prefixed. If the prefix overlaps an application route (for example, `/api`), ingress-stripped requests to that route are ambiguous; use a non-overlapping prefix such as `/reader` when the ingress strips it.

---

## 2. Kubernetes Deployment with Helm

FMD3 provides an official Helm chart located in [`charts/fmd3`](../../charts/fmd3).

### 2.1 Basic Helm Install

```bash
helm install fmd3 ./charts/fmd3 \
  --set server.ingress.enabled=true \
  --set server.ingress.host=manga.homelab.local
```

### 2.2 Enabling Distributed NATS Workers

To scale up distributed background workers across your cluster:

```bash
helm upgrade --install fmd3 ./charts/fmd3 \
  --set nats.enabled=true \
  --set worker.enabled=true \
  --set worker.replicaCount=3
```

For Helm deployments, set `server.basePath` to the same path prefix configured on the ingress, for example `--set server.basePath=/reader`. The ingress may forward the prefix unchanged or strip it before forwarding.

---

## 3. Running from Binary (Native Linux / macOS)

### 3.1 Prerequisites
- **Rust toolchain** (1.80+): Install via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **System dependencies**: `libssl-dev`, `pkg-config`

### 3.2 Building from Source

```bash
git clone https://github.com/dazedcat19/FMD2.git
cd FMD2

# Build the release binary
cargo build --release --bin fmd

# Start the web server daemon
./target/release/fmd serve --port 8088
```

---

## 4. Reproducible Environment with Nix Flake

If you use Nix or NixOS, enter an isolated development shell with all dependencies, compilers, and libraries pre-configured:

```bash
# Enter nix environment
nix develop

# Run the test suite
cargo test --workspace

# Start server
cargo run --bin fmd -- serve
```

---

## 5. First Steps in the Web Interface

1. **Verify Loaded Modules**: Go to the **Modules** tab to check that all 429 scraper modules are loaded.
2. **Configure Storage Path**: Navigate to **Settings** and ensure the download folder path and default format (**CBZ** is recommended) suit your library layout.
3. **Add Your First Manga**:
   - Go to the **Search by URL** tab.
   - Select a module (e.g. `NovelCool` or `ScanVF`).
   - Paste the direct URL of a manga series (e.g. `https://www.novelcool.com/novel/One-Piece.html`).
   - Click **Inspect Manga & Chapters**.
   - Check the chapters you want to download and click **Queue Downloads**.
4. **Track Releases**: Click the **⭐ Add to Favorites** button on any manga page to monitor newly released chapters automatically.
