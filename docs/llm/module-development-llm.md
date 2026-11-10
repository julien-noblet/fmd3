# LLM Guide: Developing FMD2 Scraper Modules (Lua & WASM)

> **Audience**: AI Coding Agents, Large Language Models, and Plugin Authors.  
> **Objective**: Generate syntactically valid, robust, and functional website scraper modules for FMD3 on the first attempt.

---

## 1. Lua Module Architecture & Contract

Every FMD2 Lua scraper module lives in `lua/modules/<ModuleName>.lua`.

### 1.1 Required Global Functions

An FMD2 Lua script must define the following entry points:

1. `Init()`: Declares module identity and capabilities.
2. `GetInfo()`: Scrapes a manga title overview page to extract metadata and chapters.
3. `GetPages()`: Scrapes a chapter reading page to extract image URLs.
4. *(Optional)* `GetCatalog()`: Crawls the website's paginated directory to populate the SQLite FTS5 catalog.

---

### 1.2 Critical Golden Rules for AI Models

> [!CAUTION]
> 1. **Relative XPath with Context Node**:  
>    When iterating over chapter or catalog nodes, **always** evaluate XPath expressions relative to the current node using a leading dot (`.`):  
>    ✅ `x.XPathString('.//a/@href', node)`  
>    ❌ `x.XPathString('//a/@href', node)`  *(Evaluating without a leading dot leaks the first node of the entire document!)*
>
> 2. **StringList Indexing**:  
>    `container.Count` is 0-indexed for bounds: `for i = 0, container.Count - 1 do`.
>
> 3. **Image Proxying / Referer Headers**:  
>    If images require specific HTTP referers to prevent HTTP 403 Forbidden, append `|Referer: <url>` to the image link string:  
>    `pages:Add(img_url .. '|Referer: ' .. chapter_url)`

---

### 1.3 Minimal Working Lua Scraper Template

When tasked with creating a new Lua scraper, strictly adhere to this template:

```lua
-- Name: ExampleManga
-- Author: FMD2 Community / AI Agent
-- Version: 1.0.0

function Init()
    -- Module Metadata Definition
    MODULE.ID          = 'example_manga'
    MODULE.Name        = 'Example Manga'
    MODULE.RootURL     = 'https://www.examplemanga.com'
    MODULE.Category    = 'English'
    MODULE.Status      = 'Active'
    MODULE.HasCatalog  = true
end

function GetInfo()
    -- URL being inspected is stored in global: URL
    local html = HTTP.Get(URL)
    x.Load(html)

    -- 1. Extract Manga Metadata
    MANGA.Title   = x.XPathString('//h1[@class="title"]/text()')
    MANGA.Cover   = x.XPathString('//div[@class="cover"]//img/@src')
    MANGA.Authors = x.XPathString('//span[@class="author"]/text()')
    MANGA.Summary = x.XPathString('//div[@class="synopsis"]//p/text()')
    MANGA.Status  = x.XPathString('//span[@class="status"]/text()')

    -- 2. Extract Chapters List
    local nodes = x.XPath('//ul[@class="chapter-list"]//li')
    for i = 0, nodes.Count - 1 do
        local node = nodes.Get(i)
        
        -- ALWAYS use dot-notation relative queries on node:
        local chap_title = x.XPathString('.//a/text()', node)
        local chap_url   = x.XPathString('.//a/@href', node)
        
        -- Clean and normalize URL
        chap_url = MaybeFillHost(MODULE.RootURL, chap_url)

        -- Append to global CHAPTERS stringlist format: "URL|TITLE"
        CHAPTERS:Add(chap_url .. '|' .. chap_title)
    end
end

function GetPages()
    -- Chapter URL being downloaded is in global: URL
    local html = HTTP.Get(URL)
    x.Load(html)

    -- Extract image URLs
    local img_nodes = x.XPath('//div[@id="reader"]//img')
    for i = 0, img_nodes.Count - 1 do
        local node = img_nodes.Get(i)
        local img_url = x.XPathString('.//@data-src', node)
        if img_url == '' then
            img_url = x.XPathString('.//@src', node)
        end

        if img_url ~= '' then
            img_url = MaybeFillHost(MODULE.RootURL, img_url)
            PAGES:Add(img_url .. '|Referer: ' .. URL)
        end
    end
end

function GetCatalog()
    -- Current page index is in global: PAGE (starts at 1)
    local cat_url = MODULE.RootURL .. '/directory?page=' .. PAGE
    local html = HTTP.Get(cat_url)
    x.Load(html)

    local items = x.XPath('//div[@class="manga-item"]')
    if items.Count == 0 then
        return -- End pagination loop
    end

    for i = 0, items.Count - 1 do
        local item = items.Get(i)
        local title = x.XPathString('.//h3/a/text()', item)
        local url   = x.XPathString('.//h3/a/@href', item)
        url = MaybeFillHost(MODULE.RootURL, url)

        -- Format: "URL|TITLE"
        CATALOG:Add(url .. '|' .. title)
    end
end
```

---

## 2. WebAssembly (WASM) Scraper Contract

Next-gen scrapers can be written in Rust and compiled targeting `wasm32-unknown-unknown`.

### 2.1 Rust WASM Module Skeleton

```rust
use std::ffi::{CStr, CString};
use std::os::raw::c_char;

#[no_mangle]
pub extern "C" fn fmd_alloc(size: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[no_mangle]
pub extern "C" fn fmd_free(ptr: *mut u8, size: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, size, size);
    }
}

#[no_mangle]
pub extern "C" fn fmd_module_info() -> u64 {
    let json = r#"{
        "id": "wasm_scraper_demo",
        "name": "WASM Demo",
        "root_url": "https://wasm.manga.demo",
        "category": "English"
    }"#;
    pack_string(json)
}

fn pack_string(s: &str) -> u64 {
    let len = s.len();
    let ptr = s.as_ptr() as u64;
    (ptr << 32) | (len as u64)
}
```
