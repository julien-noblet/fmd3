# FMD3 San — REST API & WebSocket Protocol Reference

The `fmd-server` daemon provides a comprehensive REST API and a real-time WebSocket event stream for automation, third-party integrations, and web frontends.

**Base URL**: `http://localhost:8088`

---

## 1. REST API Endpoints

### 1.1 Modules

#### `GET /api/modules`
Returns the list of all loaded website scraper modules (Lua and WASM).

**Response `200 OK`**:
```json
[
  {
    "id": "1acdfb072dc744c694a481ef0f4da6c3",
    "name": "NovelCool",
    "category": "English",
    "root_url": "https://www.novelcool.com",
    "supports_catalog": true,
    "engine": "lua"
  },
  {
    "id": "synthetic_wasm_demo",
    "name": "WASM Demo Scraper",
    "category": "WebAssembly",
    "root_url": "https://wasm.manga.local",
    "supports_catalog": false,
    "engine": "wasm"
  }
]
```

---

### 1.2 Favorites & Library

#### `GET /api/favorites`
Lists all watched manga series in the user's library with download and chapter statistics.

**Response `200 OK`**:
```json
[
  {
    "manga_id": "1acdfb072dc744c694a481ef0f4da6c3:https://www.novelcool.com/novel/One-Piece.html",
    "title": "One Piece",
    "cover_url": "https://img.novelcool.com/cover/one-piece.jpg",
    "original_url": "https://www.novelcool.com/novel/One-Piece.html",
    "module_id": "1acdfb072dc744c694a481ef0f4da6c3",
    "module_name": "NovelCool",
    "enabled": true,
    "auto_download": true,
    "download_format": "CBZ",
    "total_chapters": 1120,
    "downloaded_chapters_count": 1118,
    "last_checked_at": "2026-11-09T08:00:00Z"
  }
]
```

#### `POST /api/favorites`
Adds a manga series to the library watchlist.

**Request Payload**:
```json
{
  "module_id": "1acdfb072dc744c694a481ef0f4da6c3",
  "url": "https://www.novelcool.com/novel/One-Piece.html",
  "auto_download": true,
  "download_format": "CBZ"
}
```

#### `DELETE /api/favorites/:id`
Removes a manga from the library watchlist. Does not delete already downloaded files on disk.

#### `POST /api/favorites/check`
Triggers an immediate background check across all active favorites to discover and optionally auto-download newly published chapters.

#### `POST /api/favorites/:id/settings`
Updates tracking preferences for a specific favorite (e.g., toggling `auto_download` or changing `download_format`).

---

### 1.3 Manga Inspection & Details

#### `POST /api/manga/inspect`
Scrapes the target manga page URL and returns its metadata, description, authors, and full chapter list.

**Request Payload**:
```json
{
  "module_id": "1acdfb072dc744c694a481ef0f4da6c3",
  "url": "https://www.novelcool.com/novel/Jujutsu-Kaisen.html"
}
```

**Response `200 OK`**:
```json
{
  "manga_id": "1acdfb072dc744c694a481ef0f4da6c3:https://www.novelcool.com/novel/Jujutsu-Kaisen.html",
  "title": "Jujutsu Kaisen",
  "cover_url": "https://img.novelcool.com/covers/jjk.jpg",
  "authors": "Akutami Gege",
  "genres": "Action, Supernatural, Shounen",
  "summary": "A boy fights curses by consuming the fingers of Sukuna...",
  "status": "Ongoing",
  "chapters": [
    {
      "chapter_id": "chap_001",
      "number": 1.0,
      "title": "Ryomen Sukuna",
      "url": "https://www.novelcool.com/chapter/Jujutsu-Kaisen-Chapter-1/12345/"
    }
  ]
}
```

---

### 1.4 Downloads Queue

#### `GET /api/downloads`
Retrieves the list of active, queued, and recently finished chapter downloads.

**Response `200 OK`**:
```json
[
  {
    "id": 42,
    "manga_title": "One Piece",
    "chapter_title": "Chapter 1120",
    "module_name": "NovelCool",
    "format": "CBZ",
    "status": "Downloading",
    "total_pages": 19,
    "downloaded_pages": 12,
    "retry_count": 0,
    "error_message": null,
    "created_at": "2026-11-09T08:15:00Z"
  }
]
```

#### `POST /api/downloads/queue`
Adds one or multiple chapters to the download queue.

**Request Payload**:
```json
{
  "manga_id": "1acdfb072dc744c694a481ef0f4da6c3:https://www.novelcool.com/novel/One-Piece.html",
  "format": "CBZ",
  "chapters": [
    {
      "url": "https://www.novelcool.com/chapter/One-Piece-Chapter-1120/12345/",
      "title": "Chapter 1120",
      "number": 1120.0,
      "module_id": "1acdfb072dc744c694a481ef0f4da6c3"
    }
  ]
}
```

#### `POST /api/downloads/retry-failed`
Resets all `Failed` tasks back to `Pending` with zeroed retry counters.

#### `DELETE /api/downloads/:id`
Cancels a running task or purges a completed task from the database queue.

---

### 1.5 Catalog & FTS5 Search

#### `GET /api/catalog?query=One+Piece&module_id=all&offset=0&limit=50`
Performs full-text search query across all locally synchronized website catalogs.

#### `POST /api/catalog/sync`
Initiates a background catalog synchronization for a specific module.

**Request Payload**:
```json
{
  "module_id": "1acdfb072dc744c694a481ef0f4da6c3"
}
```

---

### 1.6 Settings & Configuration

#### `GET /api/settings`
Returns current system settings.

#### `POST /api/settings`
Updates and immediately hot-reloads runtime settings.

**Request Payload**:
```json
{
  "download_dir": "downloads",
  "default_format": "CBZ",
  "naming_template": "{manga}/{chapter}.{ext}",
  "flaresolverr_url": "http://flaresolverr:8191/v1",
  "check_interval_hours": 6,
  "catalog_resync_days": 14
}
```

---

### 1.7 Reverse Image Proxy

#### `GET /api/proxy/image?url=<encoded_url>`
Proxies external image requests through the daemon, injecting correct browser `Referer` and `User-Agent` headers to bypass Cloudflare anti-hotlink mechanisms.

---

## 2. WebSocket Protocol (`/api/ws`)

Clients connect via `ws://localhost:8088/api/ws` to receive real-time, event-driven updates.

### Event Format

Every event is a JSON object with `event` and `payload` fields:

```json
{
  "event": "<event_name>",
  "payload": { ... }
}
```

### Event Catalog

| Event Name | Description | Example Payload |
|---|---|---|
| `download_started` | A download task has begun extracting pages | `{"task_id": 42, "manga_title": "One Piece", "chapter_title": "Chap 1120"}` |
| `download_progress` | A page image was successfully fetched | `{"task_id": 42, "current_page": 12, "total_pages": 19, "percent": 63}` |
| `download_completed` | Archive packaged (CBZ/EPUB) and saved | `{"task_id": 42, "path": "downloads/One Piece/Chap 1120.cbz"}` |
| `download_failed` | Task failed after maximum retries | `{"task_id": 42, "error": "HTTP 403 Forbidden on CDN image"}` |
| `catalog_sync_started` | Module catalog synchronization started | `{"module_id": "novelcool_fr", "module_name": "NovelCool FR"}` |
| `catalog_sync_progress` | Catalog sync page progress | `{"module_id": "novelcool_fr", "current_page": 5, "items_found": 1500}` |
| `catalog_sync_completed` | Module catalog synchronization finished | `{"module_id": "novelcool_fr", "total_items": 4200}` |
| `favorite_added` | New series added to watchlist | `{"manga_id": "...", "title": "Sakamoto Days"}` |
| `favorite_updated` | New release discovered during scheduled check | `{"manga_id": "...", "new_chapters_count": 1}` |
