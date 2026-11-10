use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const STREAM_NAME: &str = "FMD_TASKS";
pub const SUBJECT_TASK_ALL: &str = "fmd.task.>";
pub const SUBJECT_TASK_DOWNLOAD: &str = "fmd.task.download";
pub const SUBJECT_TASK_SYNC: &str = "fmd.task.sync";
pub const CONSUMER_GROUP: &str = "fmd-workers";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskKind {
    DownloadChapter {
        download_id: String,
        manga_title: String,
        chapter_title: String,
        chapter_url: String,
        module_id: String,
        format: String,
        retry_count: u32,
        max_retries: u32,
    },
    SyncCatalog {
        module_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskMessage {
    pub id: String,
    pub kind: TaskKind,
    pub created_at: DateTime<Utc>,
}

impl TaskMessage {
    pub fn new_download(
        download_id: String,
        manga_title: String,
        chapter_title: String,
        chapter_url: String,
        module_id: String,
        format: String,
        retry_count: u32,
        max_retries: u32,
    ) -> Self {
        Self {
            id: download_id.clone(),
            kind: TaskKind::DownloadChapter {
                download_id,
                manga_title,
                chapter_title,
                chapter_url,
                module_id,
                format,
                retry_count,
                max_retries,
            },
            created_at: Utc::now(),
        }
    }

    pub fn new_sync(module_id: String) -> Self {
        Self {
            id: format!("sync-{}", module_id),
            kind: TaskKind::SyncCatalog { module_id },
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskProgressUpdate {
    pub task_id: String,
    pub status: String,
    pub downloaded_pages: usize,
    pub total_pages: Option<usize>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskCompleteUpdate {
    pub task_id: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub saved_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_serialization_roundtrip() {
        let task = TaskMessage::new_download(
            "dl-123".into(),
            "One Piece".into(),
            "Chapitre 1100".into(),
            "https://example.com/ch-1100".into(),
            "lelscanfr".into(),
            "cbz".into(),
            0,
            3,
        );

        let json = serde_json::to_string(&task).expect("serialize task");
        let decoded: TaskMessage = serde_json::from_str(&json).expect("deserialize task");

        assert_eq!(task.id, decoded.id);
        match decoded.kind {
            TaskKind::DownloadChapter {
                manga_title,
                format,
                retry_count,
                ..
            } => {
                assert_eq!(manga_title, "One Piece");
                assert_eq!(format, "cbz");
                assert_eq!(retry_count, 0);
            }
            _ => panic!("Expected DownloadChapter variant"),
        }
    }
}
