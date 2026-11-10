use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MangaInfoResult {
    pub title: String,
    pub original_url: String,
    pub cover_url: Option<String>,
    pub authors: Option<String>,
    pub artists: Option<String>,
    pub genres: Vec<String>,
    pub status: String,
    pub summary: Option<String>,
    pub chapters: Vec<ChapterItem>,
    pub alternate_versions: Vec<AlternateVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternateVersion {
    pub language: String,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterItem {
    pub title: String,
    pub url: String,
}
