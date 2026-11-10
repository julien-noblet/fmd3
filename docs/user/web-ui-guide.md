# FMD3 San — Web UI User Guide

The FMD3 Web UI is a responsive Single Page Application rendered via Leptos SSR and enhanced with real-time WebSocket notifications.

---

## 1. Top Navigation & Header

The top navigation bar provides quick access to all functional areas of FMD2:

- **Dashboard Statistics**:
  - **Séries Suivies (Favorites)**: Total number of manga series monitored in your library.
  - **Catalogue Indexé (FTS5)**: Total count of indexed manga series across all synchronized sites.
  - **Téléchargements**: Total tasks in queue and actively downloading count.
  - **Modules Actifs**: Number of discovered scrapers (429+).
- **Live Status Pill**: Indicates real-time WebSocket connectivity (`Connected (WS)` or reconnecting).

---

## 2. Tabs Overview

### 2.1 Library & Watched Favorites (`Bibliothèque & Favoris`)
The Library tab displays all manga series you are tracking:

- **Card View**: Displays the series cover, title, origin module, and chapter progress counter (e.g. `12 / 120 chap.`).
- **Auto-Download Toggle (`⚡ Auto-DL: ON / OFF`)**: When active, newly published chapters discovered during background checks are queued and downloaded automatically without user intervention.
- **Tracking Status**: Toggle between *Active Monitoring* and *Paused*.
- **Check All Favorites (`🔄 Vérifier tous les favoris`)**: Instantly triggers a background check against all target websites to look for newly published chapters.

---

### 2.2 Catalog & Full-Text Search (`Catalogue & Recherche`)
Browse and search through tens of thousands of manga series stored in your local SQLite FTS5 index:

- **Full-Text Search Bar**: Instant search with fuzzy matching and partial prefix matching (e.g. `Jujutsu`, `Solo Leveling`, `Berserk`).
- **Module Filter Dropdown**: Filter results by a specific website or search across all indexed sites simultaneously.
- **Synchronize Button (`🔄 Synchroniser le catalogue`)**: Starts a background crawler for the selected module to fetch its entire catalog and index it into SQLite FTS5.
- **Sync Progress Banner**: Shows real-time page-by-page progress and discovery counters while a sync is in progress.

---

### 2.3 Direct URL Inspection (`+ Ajouter par URL`)
Use this tab to download chapters directly from an URL or add a new manga to your library:

1. **Select Site Module**: Choose the source module from the dropdown.
2. **Paste Direct URL**: Enter the web address of the manga overview page (e.g. `https://fr.novelcool.com/novel/One-Piece.html`).
3. **Inspect**: Click **Charger les chapitres et infos du manga**.
4. **Metadata Preview**: Review the cover, synopsis, authors, status, and detected chapters.
5. **Add to Favorites (`⭐ Ajouter à mes favoris`)**: Saves the series to your watched library.
6. **Chapter Selection Controls**:
   - Quick selection buttons: *Check All*, *Uncheck All*, *Invert Selection*, *Last 10*, *Last 25*.
   - Output format selector: **CBZ**, **PDF**, **EPUB**, or **Raw Image Directory**.
7. **Queue Download**: Click **📥 Télécharger** to start downloading the selected chapters.

---

### 2.4 Multi-Source Merger (`⚡ Fusion Multi-Sources`)
When reading a long-running manga series where one website has the complete early archive and another has faster updates for recent chapters:

- Enter the canonical manga title.
- Inspect and align chapters from **Source A** (e.g. official translation) and **Source B** (e.g. community archive).
- Click **Combler les manques (Gap-Filling)**: Automatically selects missing chapters from the secondary source.
- All downloaded files converge into the exact same folder structure (`downloads/{manga}/{chapter}.cbz`).

---

### 2.5 Downloads Queue (`Téléchargements`)
Real-time monitoring of all active and queued operations:

- **Table View**: Lists manga title, chapter, module source, target format, downloaded pages counter, and status.
- **Status Badges**:
  - `Pending`: Waiting in queue.
  - `Downloading`: Actively fetching page images from CDN.
  - `Extracting` / `Compressing`: Building the final CBZ or EPUB archive with `ComicInfo.xml`.
  - `Completed`: File packaged and saved to disk.
  - `Failed`: Error encountered (e.g. CDN timeout or missing images).
- **Retry Failed Button (`🔄 Relancer les échecs`)**: Retries all failed chapters with a single click.

---

### 2.6 Modules Explorer (`Modules`)
A searchable table of all 429+ website scrapers:

- View module name, language/category, and root website URL.
- Click **📖 Explorer** to jump directly to its indexed catalog.
- Click **URL →** to pre-fill the URL inspector with this module.

---

### 2.7 Settings (`⚙️ Réglages`)
Configure system preferences applied on the fly:

- **Download Folder**: Host filesystem path for storing archives (e.g. `downloads` or `/data/mangas`).
- **Default Format**: Default packaging format for automated downloads (**CBZ** recommended).
- **Naming Pattern**: Customize directory and file structures (e.g. `{manga}/{chapter}.{ext}`).
- **FlareSolverr URL**: Point to an optional FlareSolverr proxy instance to defeat Cloudflare anti-bot checks.
- **Check Intervals**: Scheduled recurrence for checking favorite manga (every 1h, 3h, 6h, 12h, or 24h).
