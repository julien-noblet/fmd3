use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manga {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub original_url: String,
    pub cover_url: Option<String>,
    pub author: Option<String>,
    pub artist: Option<String>,
    pub description: Option<String>,
    pub genres: Vec<String>,
    pub status: MangaStatus,
    pub added_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MangaStatus {
    #[default]
    Unknown,
    Ongoing,
    Completed,
    Licensed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub manga_id: String,
    pub title: String,
    pub chapter_number: f32,
    pub volume_number: Option<f32>,
    pub url: String,
    pub release_date: Option<DateTime<Utc>>,
    pub is_downloaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PackagingFormat {
    #[default]
    Directory,
    Cbz,
    Zip,
    Pdf,
    Epub,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub id: Uuid,
    pub manga_title: String,
    pub chapter_title: String,
    pub chapter_url: String,
    pub module_id: String,
    pub format: PackagingFormat,
    pub total_pages: usize,
    pub downloaded_pages: usize,
    pub status: TaskStatus,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    Downloading,
    Packaging,
    Completed,
    Failed,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogItem {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub link: String,
    pub cover_url: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSyncStatus {
    pub module_id: String,
    pub total_mangas: i64,
    pub status: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterItem {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Favorite {
    pub manga_id: String,
    pub title: String,
    pub module_id: String,
    pub original_url: String,
    pub cover_url: Option<String>,
    pub order_index: i64,
    pub enabled: bool,
    pub auto_download: bool,
    pub download_format: String,
    pub current_chapter: Option<String>,
    pub save_to: Option<String>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub last_updated_at: Option<DateTime<Utc>>,
    pub total_chapters: i64,
    pub downloaded_chapters_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub download_dir: String,
    pub default_format: String,
    pub naming_template: String,
    pub flaresolverr_url: String,
    pub check_interval_hours: u32,
    pub catalog_resync_days: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            download_dir: "downloads".to_string(),
            default_format: "CBZ".to_string(),
            naming_template: "{manga}/{chapter}".to_string(),
            flaresolverr_url: "".to_string(),
            check_interval_hours: 1,
            catalog_resync_days: 7,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChapterMetadata {
    pub series: String,
    pub title: String,
    pub number: Option<String>,
    pub summary: Option<String>,
    pub writer: Option<String>,
    pub genres: Vec<String>,
    pub web: Option<String>,
    pub language_iso: Option<String>,
}
