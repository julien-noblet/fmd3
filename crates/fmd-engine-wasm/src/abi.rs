use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmModuleMeta {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub root_url: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmSearchQuery {
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmCatalogItem {
    pub title: String,
    pub link: String,
    pub cover_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmSearchResult {
    pub items: Vec<WasmCatalogItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmMangaInfoQuery {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmChapterItem {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmMangaInfoResult {
    pub title: String,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub genres: Vec<String>,
    pub status: Option<String>,
    pub chapters: Vec<WasmChapterItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmPagesQuery {
    pub chapter_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmPagesResult {
    pub pages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmImageProcessQuery {
    pub filename: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WasmImageProcessResult {
    pub success: bool,
    pub format: String,
    pub error: Option<String>,
}
