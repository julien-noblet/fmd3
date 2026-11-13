# Repository guide

FMD3 is a Rust 2021 Cargo workspace for a manga downloader, server, and web UI. The README and `docs/` are the source of truth for user-facing behavior and detailed subsystem contracts.

## Build, test, and lint

Run these from the repository root:

```bash
cargo build --workspace
cargo test --workspace
cargo test -p fmd-net --test hermetic_integration_test test_hermetic_end_to_end_scraper_and_cbz_packaging
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

For a focused Rust test, use `cargo test -p <crate> <test-name-filter>`. The workspace tests are intended to be hermetic; Lua tests requiring live external sites are explicitly ignored. CI runs formatting, Clippy, and workspace tests as shown in `.github/workflows/ci.yml`.

## Development environment

The Nix flake provides the Rust toolchain, rust-analyzer, Clippy, rustfmt, pkg-config, OpenSSL, and SQLite. Enter it with `nix develop`; `.envrc` runs `use flake` for direnv, so approve it once with `direnv allow` and the environment loads automatically when entering the repository. The shell sets `RUST_BACKTRACE=1`.

## Architecture

- `fmd-cli` is the `fmd` executable and wires the server, engines, and worker commands together.
- `fmd-server` provides the Axum API and WebSocket updates; `fmd-ui` supplies the Leptos SSR interface.
- `fmd-core` owns shared domain models, SQLite persistence, task contracts, merging, and archive packaging.
- `fmd-engine-lua` runs legacy FMD2 Lua scrapers by emulating their Pascal/Lazarus globals and helpers. `fmd-engine-wasm` runs sandboxed modules through Wasmtime.
- `fmd-net` provides shared HTTP behavior. Download tasks flow through the local queue or NATS JetStream to a worker, which invokes a scraper, fetches pages, and uses `fmd-core` to package output.
- Workspace dependencies are declared in the root `Cargo.toml`; crate manifests should refer to shared dependencies with `workspace = true`.

For cross-crate dependencies and data flow, see [`docs/technical/architecture.md`](../docs/technical/architecture.md). For engine contracts, see [`docs/technical/scraping-engines.md`](../docs/technical/scraping-engines.md).

## Scraper conventions

Lua site modules live in `lua/modules/`; shared site-family implementations live in `lua/templates/`. A module's `Init()` declares metadata and callbacks; its event functions extract manga information, chapters, and pages. Consult [`docs/llm/module-development-llm.md`](../docs/llm/module-development-llm.md) for the full Lua and WASM contracts.

- When querying an XPath from a selected node, use a relative expression beginning with `.` (for example, `x.XPathString('.//a/@href', node)`) so the query stays within that node.
- `StringList.Count` is used with zero-based bounds: iterate from `0` through `Count - 1`.
- Preserve the established list formats: chapter/catalog entries are `URL|TITLE`; image URLs that need a referer use `URL|Referer: <url>`.
- Prefer hermetic tests with local mocks for network behavior. Live-site Lua tests are ignored because they require external network access.
