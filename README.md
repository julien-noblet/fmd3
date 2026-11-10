<div align="center">

<img src="images/logo.png" alt="FMD3 San Logo" width="220" />

# Free Manga Downloader 3 — San (FMD3 / FMD三)
### High-Performance Manga Downloader, Headless Daemon & Web UI in Native Rust

[![CI/CD](https://github.com/julien-noblet/fmd3/actions/workflows/ci.yml/badge.svg)](.github/workflows/ci.yml)
[![License: GPL-2.0](https://img.shields.io/badge/License-GPL%20v2-blue.svg)](license.txt)
[![Rust](https://img.shields.io/badge/Rust-2021_Edition-orange.svg?logo=rust)](Cargo.toml)
[![Leptos](https://img.shields.io/badge/UI-Leptos_0.7_SSR-red.svg)](crates/fmd-ui)
[![WebAssembly](https://img.shields.io/badge/Sandbox-Wasmtime_29-blueviolet.svg?logo=webassembly)](crates/fmd-engine-wasm)
[![Distributed](https://img.shields.io/badge/Queue-NATS_JetStream-27AAE1.svg?logo=nats.io)](crates/fmd-core)

</div>

A high-performance, headless daemon, CLI, and modern Web UI rewrite known as **FMD3 San (三)** (*San* standing for 3 in Japanese, honoring the iconic manga culture and trilogy evolution) in **Rust**, designed for Linux servers, Docker containers, Kubernetes homelabs, and desktop environments.

---

## 🌟 Key Features

- **🚀 100% Native Rust Architecture**
  - High concurrency with Tokio async I/O.
  - Safe, minimal memory footprint, replacing the legacy Pascal/Lazarus codebase.
- **🧩 Dual Scraper Engine (Lua + WASM)**
  - **429+ website scraper modules supported out-of-the-box** with full emulation of HTTP, XPath (`TXQuery`), HTML parsing, crypto, and string utilities.
  - **WebAssembly Sandboxing (Wasmtime)** for next-generation scrapers and image processing filters with fuel-metering protection.
- **🖥️ Modern Leptos Web UI & REST API**
  - Fast, responsive server-side rendered (SSR) interface with real-time WebSocket progress updates.
  - Full-text catalog search (**SQLite FTS5**) across tens of thousands of manga series.
  - Interactive multi-source merger with automatic gap-filling.
- **📦 ComicInfo.xml & E-Reader Packaging**
  - Automatic injection of standard `ComicInfo.xml` metadata into **CBZ** and **CBR** archives.
  - 100% compatible with **Komga**, **Kavita**, **Mihon**, and **Suwayomi**.
  - Flexible export formats: **CBZ**, **EPUB** (with Dublin Core metadata), **PDF**, or raw numbered directories.
- **⚡ Distributed & Scalable (NATS JetStream)**
  - Embedded background queue for single-instance setups.
  - Pluggable distributed worker architecture via **NATS JetStream** (`fmd worker`) for massive parallel downloads.
- **☸️ Cloud-Native & Homelab Ready**
  - Ready-to-deploy **Docker Compose** stack.
  - Complete **Helm Chart** (`charts/fmd3`) with configurable PVCs, Ingress, and auto-scaling workers.
  - Deterministic Nix Flake development environment (`nix develop`).

---

## 🚀 Quick Start

### 1. Using Pre-built Docker Compose

```bash
docker compose up -d
```
Open your browser at **http://localhost:8088** to access the Web UI.

### 2. Building from Source (Rust Cargo)

Make sure you have Rust 1.80+ or use Nix Flake:

```bash
# Enter nix shell (optional)
nix develop

# Build and start server
cargo run --release --bin fmd -- serve --port 8088 --bind 0.0.0.0
```

---

## 💻 CLI Commands

The `fmd` binary offers a powerful command-line interface for headless automation:

```bash
# Start server daemon
fmd serve --port 8088

# List all discovered scraper modules (Lua & WASM)
fmd modules

# Diagnose and test a specific module with an URL
fmd modules --check MangaPlus --url "https://mangaplus.shueisha.co.jp/titles/100020"

# List tracked favorites and download status
fmd favorites --list

# Check favorites for newly released chapters
fmd favorites --check --auto-download

# Search catalog with full-text search (FTS5)
fmd catalog --query "One Piece"

# Synchronize catalog for a specific module
fmd catalog --sync novelcool_fr

# Import legacy FMD2 userdata.db / SQLite databases
fmd import-legacy --db path/to/userdata.db

# Run as a distributed background worker connected to NATS
fmd worker --nats nats://localhost:4222
```

---

## 🏗️ Architecture & Crates

```text
├── charts/fmd3/             # Kubernetes Helm Chart (Server, Worker, NATS, PVC, Ingress)
├── crates/
│   ├── fmd-cli/             # Main executable binary (fmd serve, modules, favorites, worker)
│   ├── fmd-core/            # SQLite schema, ComicInfo.xml packer, puzzle descramblers, NATS queue
│   ├── fmd-engine-lua/      # Lua 5.4 VM, scraper runtime (HTTP, TXQuery, Duktape, crypto)
│   ├── fmd-engine-wasm/     # Wasmtime sandbox, ABI guest contracts, fuel metering
│   ├── fmd-net/             # HTTP client, Cloudflare bypass hooks, WireMock hermetic tests
│   ├── fmd-server/          # Axum HTTP API, WebSocket live streaming, embedded Web UI
│   └── fmd-ui/              # Leptos 0.7 SSR frontend components and views
└── lua/modules/             # 429+ website scraper modules (.lua)
```

---

## 📚 Documentation

The complete documentation suite is available in the [`docs/`](docs/) directory and via the standard [`llms.txt`](llms.txt) index:

- **[Documentation Portal](docs/index.md)**: Main landing page and cross-referenced index.
- **Technical Architecture**:
  - [Architecture & Crates](docs/technical/architecture.md) — 7-crate design, lifecycle, and data flows.
  - [REST API & WebSocket Reference](docs/technical/api-reference.md) — Endpoints, payloads, and WebSocket events.
  - [Scraping Engines](docs/technical/scraping-engines.md) — Lua 5.4 Pascal emulation, C Protobuf bindings, and Wasmtime ABI.
  - [Distributed NATS Queue](docs/technical/distributed-queue.md) — JetStream clustering and pull worker groups.
  - [Database Schema & FTS5](docs/technical/database-schema.md) — SQLite schema, WAL mode, and full-text search.
- **User Guides**:
  - [Getting Started Guide](docs/user/getting-started.md) — Docker Compose, Helm K8s, Cargo, and Nix Flake.
  - [Web UI Walkthrough](docs/user/web-ui-guide.md) — Interactive features, catalog search, and multi-source merging.
  - [CLI Reference Manual](docs/user/cli-reference.md) — Full manual for all `fmd` subcommands and flags.
  - [Configuration & Metadata](docs/user/configuration.md) — ComicInfo.xml, CBZ/EPUB formatting, and FlareSolverr.
- **AI & LLM Integration**:
  - [Root LLM Index (llms.txt)](llms.txt) — Fast open standard discovery file for LLMs.
  - [Consolidated Context (llms-full.txt)](llms-full.txt) — Single complete context file for direct prompt injection.
  - [LLM Module Development Guide](docs/llm/module-development-llm.md) — Rules, templates, and patterns for generating scrapers.
  - [Autonomous Agent Integration](docs/llm/agent-integration.md) — JSON schemas and agent orchestration workflows.

---

## ☸️ Kubernetes Deployment

Deploy on any Kubernetes cluster (k3s, microk8s, EKS, GKE) with Helm:

```bash
helm install fmd3 ./charts/fmd3 \
  --set server.ingress.enabled=true \
  --set server.ingress.host=fmd.homelab.local
```

---

## 🧪 Testing & Verification

Run the hermetic integration test suite across the entire workspace:

```bash
cargo test --workspace
```

Continuous integration runs on GitHub Actions on every pull request with formatting, Clippy, hermetic WireMock tests, and multi-architecture Docker container builds (`linux/amd64`, `linux/arm64`).

---

## 📜 License

This project is licensed under the **GNU General Public License v2 (GPL-2.0)** to preserve compatibility with the original Free Manga Downloader project. See [`license.txt`](license.txt) for details.
