pub mod auth;
pub mod cache;
pub mod db;
pub mod errors;
pub mod merger;
pub mod models;
pub mod packer;
pub mod puzzle;
pub mod queue;

pub use auth::{ApiToken, AuthContext, Role, User, UserPermissions, UserSession};
pub use cache::{MangaCacheItem, ModuleCachePolicy, TtlUnit};
pub use db::Database;
pub use errors::{Error, Result};
pub use merger::{
    detect_chapter_gaps, merge_chapter_sources, parse_chapter_number_value, ChapterGap,
    MergedChapterItem, MergedMangaResult, SourceRef,
};
pub use models::{
    AppSettings, CatalogItem, ChapterItem, ChapterMetadata, Favorite, ModuleSyncStatus,
    PackagingFormat,
};
pub use packer::{extract_chapter_number, format_download_path, ChapterPacker};
pub use queue::{
    TaskCompleteUpdate, TaskKind, TaskMessage, TaskProgressUpdate, CONSUMER_GROUP, STREAM_NAME,
    SUBJECT_TASK_ALL, SUBJECT_TASK_DOWNLOAD, SUBJECT_TASK_SYNC,
};
