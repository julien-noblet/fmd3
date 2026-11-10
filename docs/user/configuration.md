# FMD3 San — Configuration, Formats & Metadata Specification

This guide explains how to customize output formats, configure `ComicInfo.xml` metadata for e-reader servers (Komga, Kavita, Mihon), and tune Cloudflare bypass settings.

---

## 1. Output Packaging Formats

FMD3 supports four distinct packaging formats for downloaded chapters:

| Format | File Extension | Description | Recommended Use Case |
|---|---|---|---|
| **CBZ** | `.cbz` | ZIP archive containing zero-padded images and `ComicInfo.xml` | **Recommended**. Best for Komga, Kavita, Mihon, YACReader |
| **EPUB** | `.epub` | Standard e-book format with XHTML spine and Dublin Core metadata | Best for Kobo, Kindle, and Apple Books e-readers |
| **PDF** | `.pdf` | Multi-page PDF document | Best for document viewers and desktop reading |
| **Directory** | *(folder)* | Raw uncompressed directory containing downloaded numbered image files | Best for custom scripting or manual image editing |

---

## 2. Standard `ComicInfo.xml` Metadata

For every **CBZ** archive generated, FMD3 automatically constructs and injects a standard `ComicInfo.xml` document at the archive root.

### 2.1 E-Reader Compatibility
This XML file follows the standard Manga/Comic metadata schema supported by:
- **Komga**
- **Kavita**
- **Mihon (formerly Tachiyomi)**
- **Suwayomi**
- **YACReader**

### 2.2 Sample Generated `ComicInfo.xml`

```xml
<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema">
  <Series>One Piece</Series>
  <Number>1120</Number>
  <Title>Atlas</Title>
  <Summary>The Straw Hat crew continues their escape from Egghead Island while Vegapunk reveals world secrets.</Summary>
  <Writer>Eiichiro Oda</Writer>
  <Genre>Action, Adventure, Fantasy, Shounen</Genre>
  <Web>https://www.novelcool.com/novel/One-Piece.html</Web>
  <Manga>YesAndRightToLeft</Manga>
  <PageCount>19</PageCount>
</ComicInfo>
```

> [!NOTE]
> The `<Manga>YesAndRightToLeft</Manga>` tag instructs reader applications to automatically configure the reading direction from right-to-left.

---

## 3. Custom Path Naming Templates

You can customize where and how files are stored using dynamic template variables in **Settings** (`setting-naming-template`).

### 3.1 Template Variables

| Variable | Description | Example Value |
|---|---|---|
| `{manga}` | Sanitized title of the manga series | `One Piece` |
| `{chapter}` | Formatted chapter title or number | `Chapter 1120` |
| `{title}` | Chapter title string | `Atlas` |
| `{number}` | Numerical chapter value | `1120` |
| `{ext}` | Target file extension (without dot) | `cbz` |
| `{format}` | Format name | `CBZ` |

### 3.2 Common Patterns

- **Standard Flat Organization** (Default):
  ```text
  {manga}/{chapter}.{ext}
  --> downloads/One Piece/Chapter 1120.cbz
  ```
- **Volume Grouping**:
  ```text
  {manga}/Vol {number}/{chapter}.{ext}
  --> downloads/One Piece/Vol 105/Chapter 1120.cbz
  ```
- **Verbose Naming**:
  ```text
  {manga}/[NovelCool] {manga} - {chapter} - {title}.{ext}
  --> downloads/One Piece/[NovelCool] One Piece - Chapter 1120 - Atlas.cbz
  ```

---

## 4. FlareSolverr & Cloudflare Bypass

Websites protected by Cloudflare Turnstile or aggressive browser challenges can be resolved by connecting FMD2 to a **FlareSolverr** proxy instance.

### 4.1 Running FlareSolverr with Docker

Add FlareSolverr to your `docker-compose.yml`:

```yaml
services:
  flaresolverr:
    image: ghcr.io/flaresolverr/flaresolverr:latest
    container_name: flaresolverr
    environment:
      - LOG_LEVEL=info
      - PORT=8191
    restart: unless-stopped
```

### 4.2 Configuring FMD2

In FMD2 **Settings** (or via the REST API / `fmd config`), set:

```text
flaresolverr_url = http://flaresolverr:8191/v1
```

When a scraper encounters a Cloudflare challenge, FMD2 dispatches the URL to FlareSolverr, obtains valid clearance cookies and browser `User-Agent` strings, and persists them into the module's session cache.
