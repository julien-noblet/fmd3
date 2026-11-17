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

/// Optional runtime overrides loaded from environment variables.
/// When set, they take precedence over values stored in the database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuntimeEnvOverrides {
    pub min_delay_ms: Option<u64>,
    pub check_interval_hours: Option<u32>,
    pub catalog_resync_days: Option<u32>,
}

impl RuntimeEnvOverrides {
    pub fn from_env() -> Result<Self, String> {
        fn read(name: &str) -> Result<Option<String>, String> {
            match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(err) => Err(format!("Invalid {name}: {err}")),
            }
        }

        let min_delay_ms = read("FMD_MIN_DELAY_MS")?;
        let check_interval_hours = read("FMD_CHECK_INTERVAL_HOURS")?;
        let catalog_resync_days = read("FMD_CATALOG_RESYNC_DAYS")?;

        Self::parse(
            min_delay_ms.as_deref(),
            check_interval_hours.as_deref(),
            catalog_resync_days.as_deref(),
        )
    }

    pub fn parse(
        min_delay_ms: Option<&str>,
        check_interval_hours: Option<&str>,
        catalog_resync_days: Option<&str>,
    ) -> Result<Self, String> {
        fn parse_u64(name: &str, raw: Option<&str>) -> Result<Option<u64>, String> {
            let Some(val) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
                return Ok(None);
            };
            val.parse::<u64>()
                .map(Some)
                .map_err(|_| format!("{name} must be a non-negative integer, got {val:?}"))
        }

        fn parse_positive_u32(name: &str, raw: Option<&str>) -> Result<Option<u32>, String> {
            let Some(val) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
                return Ok(None);
            };
            let parsed = val
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a positive integer, got {val:?}"))?;
            if parsed == 0 {
                return Err(format!("{name} must be a positive integer (>= 1)"));
            }
            Ok(Some(parsed))
        }

        Ok(Self {
            min_delay_ms: parse_u64("FMD_MIN_DELAY_MS", min_delay_ms)?,
            check_interval_hours: parse_positive_u32(
                "FMD_CHECK_INTERVAL_HOURS",
                check_interval_hours,
            )?,
            catalog_resync_days: parse_positive_u32(
                "FMD_CATALOG_RESYNC_DAYS",
                catalog_resync_days,
            )?,
        })
    }

    pub fn apply_to_settings(&self, settings: &mut AppSettings) {
        if let Some(hours) = self.check_interval_hours {
            settings.check_interval_hours = hours;
        }
        if let Some(days) = self.catalog_resync_days {
            settings.catalog_resync_days = days;
        }
    }

    pub fn effective_min_delay_ms(&self, policy_delay_ms: u64) -> u64 {
        self.min_delay_ms.unwrap_or(policy_delay_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_env_overrides() {
        let parsed = RuntimeEnvOverrides::parse(Some("2500"), Some("6"), Some("14")).unwrap();
        assert_eq!(parsed.min_delay_ms, Some(2500));
        assert_eq!(parsed.check_interval_hours, Some(6));
        assert_eq!(parsed.catalog_resync_days, Some(14));

        let zero_delay = RuntimeEnvOverrides::parse(Some("0"), None, Some("")).unwrap();
        assert_eq!(zero_delay.min_delay_ms, Some(0));
        assert_eq!(zero_delay.check_interval_hours, None);
        assert_eq!(zero_delay.catalog_resync_days, None);

        let mut settings = AppSettings::default();
        parsed.apply_to_settings(&mut settings);
        assert_eq!(settings.check_interval_hours, 6);
        assert_eq!(settings.catalog_resync_days, 14);
        assert_eq!(parsed.effective_min_delay_ms(1000), 2500);
        assert_eq!(zero_delay.effective_min_delay_ms(1000), 0);
    }

    #[test]
    fn rejects_invalid_runtime_env_overrides() {
        assert!(RuntimeEnvOverrides::parse(Some("-1"), None, None).is_err());
        assert!(RuntimeEnvOverrides::parse(Some("abc"), None, None).is_err());
        assert!(RuntimeEnvOverrides::parse(None, Some("0"), None).is_err());
        assert!(RuntimeEnvOverrides::parse(None, Some("abc"), None).is_err());
        assert!(RuntimeEnvOverrides::parse(None, None, Some("0")).is_err());
        assert!(RuntimeEnvOverrides::parse(None, None, Some("xyz")).is_err());
    }
}
