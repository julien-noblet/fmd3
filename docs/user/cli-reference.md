# FMD3 San — CLI Reference Manual

The `fmd` executable provides a command-line interface for headless automation, server administration, cron jobs, and distributed background workers.

---

## Global Options

```bash
fmd [OPTIONS] <COMMAND>

Options:
  -h, --help       Print help information
  -V, --version    Print version information
```

---

## 1. `fmd serve`

Starts the headless daemon, HTTP REST API, and embedded Web UI.

```bash
fmd serve [OPTIONS]

Options:
  -p, --port <PORT>        Port to bind the web server [default: 8088]
  -b, --bind <BIND>        Network address to listen on [default: 127.0.0.1]
      --db <PATH>          Path to the SQLite database [default: data/fmd.db]
  -d, --dir <DIR>          Directory containing scraper modules [default: lua/modules]
      --nats <NATS_URL>    NATS JetStream broker URL (enables distributed mode)
  -h, --help               Print help
```

### Examples
```bash
# Start server listening on all interfaces on port 8088
fmd serve --bind 0.0.0.0 --port 8088

# Start server connected to a distributed NATS cluster
fmd serve --nats nats://nats.homelab.local:4222
```

---

## 2. `fmd modules`

Scans, lists, diagnoses, and validates website scraper modules.

```bash
fmd modules [OPTIONS]

Options:
  -d, --dir <DIR>          Directory containing modules [default: lua/modules]
  -c, --check <MODULE>     Diagnose a specific module by name
  -u, --url <URL>          Target manga URL to test with --check
  -h, --help               Print help
```

### Examples
```bash
# Discover and list all loaded modules
fmd modules

# Test and diagnose MangaPlus scraper against a live title
fmd modules --check MangaPlus --url "https://mangaplus.shueisha.co.jp/titles/100020"
```

---

## 3. `fmd favorites`

Manages tracked manga, checks for new releases, and queues automated downloads.

```bash
fmd favorites [OPTIONS]

Options:
  -l, --list               List all tracked manga with chapter statistics
  -c, --check              Check monitored manga for newly released chapters
      --id <ID>            Target specific manga ID or title to check
      --auto-download      Automatically queue downloads for newly detected chapters
      --db <PATH>          Path to SQLite database [default: data/fmd.db]
  -d, --dir <DIR>          Modules directory [default: lua/modules]
  -h, --help               Print help
```

### Examples
```bash
# Display watched library table
fmd favorites --list

# Check all favorites for new chapters and download them immediately (ideal for cron jobs)
fmd favorites --check --auto-download

# Check only a single series
fmd favorites --check --id "One Piece" --auto-download
```

---

## 4. `fmd catalog`

Interacts with the SQLite FTS5 catalog and triggers module scraping synchronization.

```bash
fmd catalog [OPTIONS]

Options:
  -q, --query <QUERY>      Full-text search query
  -m, --module <MODULE_ID> Filter by module ID
  -s, --sync <MODULE_ID>   Trigger full catalog crawler for a specific module
      --db <PATH>          Path to SQLite database [default: data/fmd.db]
  -h, --help               Print help
```

### Examples
```bash
# Search catalog for "Jujutsu Kaisen"
fmd catalog --query "Jujutsu"

# Crawl and synchronize the entire catalog of NovelCool
fmd catalog --sync novelcool_fr
```

---

## 5. `fmd worker`

Runs a standalone, headless background worker connected to NATS JetStream.

```bash
fmd worker [OPTIONS] --internal-token <INTERNAL_TOKEN>

Options:
      --nats-url <NATS_URL>              NATS server URL [env: FMD_NATS_URL=] [default: nats://127.0.0.1:4222]
      --server-url <SERVER_URL>          FMD3 Server API URL [env: FMD_SERVER_URL=] [default: http://127.0.0.1:8088]
  -c, --concurrency <CONCURRENCY>        Concurrency level (concurrent tasks) [default: 2]
  -d, --dir <DIR>                        Directory containing Lua modules [default: lua/modules]
      --downloads-dir <DOWNLOADS_DIR>    Directory for downloaded archives [default: downloads]
      --internal-token <INTERNAL_TOKEN>  Internal worker authentication token (required, non-empty) [env: FMD_INTERNAL_TOKEN=]
  -h, --help                             Print help
```

> [!IMPORTANT]
> `fmd worker` refuses to start if `FMD_INTERNAL_TOKEN` / `--internal-token` is unset or empty. Both `fmd serve` and `fmd worker` must share the same `FMD_INTERNAL_TOKEN` value so internal progress and completion callbacks succeed.

### Example
```bash
# Launch a background worker with shared internal authentication token
export FMD_INTERNAL_TOKEN="replace-with-shared-secret"
fmd worker --nats-url nats://10.0.0.10:4222 --server-url http://10.0.0.10:8088 --concurrency 8
```

---

## 6. `fmd import-legacy`

Imports favorites, downloaded chapter histories, and settings from a legacy Windows FMD2 SQLite database (`userdata.db`).

```bash
fmd import-legacy --db path/to/userdata.db
```
