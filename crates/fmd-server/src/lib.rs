mod base_path;
pub mod ui;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    response::{Html, IntoResponse},
    routing::{delete, get, post, put},
    Json, Router,
};
use fmd_core::Database;
use fmd_engine_lua::{LuaModuleEngine, MangaInfoResult, WebsiteModule};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch, Mutex};
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

pub mod auth;
mod demo;

const LOGO_PNG: &[u8] = include_bytes!("../../../images/logo-256.png");
const FAVICON_ICO: &[u8] = include_bytes!("../../../images/favicon.ico");
const FAVICON_32: &[u8] = include_bytes!("../../../images/favicon-32x32.png");
const FAVICON_16: &[u8] = include_bytes!("../../../images/favicon-16x16.png");
const APPLE_TOUCH_ICON: &[u8] = include_bytes!("../../../images/apple-touch-icon.png");

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub shared_cache: Option<Database>,
    pub demo_shutdown: Option<watch::Receiver<bool>>,
    pub engine: Arc<Mutex<LuaModuleEngine>>,
    pub modules: Arc<Vec<WebsiteModule>>,
    pub ws_sender: broadcast::Sender<WsNotification>,
    pub catalog_ws_sender: Option<broadcast::Sender<WsNotification>>,
    pub sync_manager: Arc<CatalogSyncManager>,
    pub nats_context: Option<async_nats::jetstream::Context>,
    pub throttler: auth::DomainThrottler,
    pub login_rate_limiter: Arc<auth::LoginRateLimiter>,
    pub internal_worker_token: String,
}

impl AppState {
    fn cache_db(&self) -> &Database {
        self.shared_cache.as_ref().unwrap_or(&self.db)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WsNotification {
    pub event: String,
    pub payload: serde_json::Value,
}

pub(crate) fn send_catalog_notification(
    private_ws: &broadcast::Sender<WsNotification>,
    catalog_ws: Option<&broadcast::Sender<WsNotification>>,
    notification: WsNotification,
) {
    let sender = catalog_ws.unwrap_or(private_ws);
    let _ = sender.send(notification);
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveSyncInfo {
    pub module_id: String,
    pub module_name: String,
    pub current_directory: usize,
    pub total_directories: usize,
    pub current_page: usize,
    pub total_pages: Option<usize>,
    pub items_found: usize,
    pub started_at_secs: u64,
}

#[derive(Default)]
pub struct CatalogSyncManager {
    active_cancel: Mutex<Option<(String, Arc<std::sync::atomic::AtomicBool>)>>,
    active_info: Mutex<Option<ActiveSyncInfo>>,
}

impl CatalogSyncManager {
    pub fn new() -> Self {
        Self {
            active_cancel: Mutex::new(None),
            active_info: Mutex::new(None),
        }
    }

    pub async fn is_syncing(&self) -> bool {
        self.active_cancel.lock().await.is_some()
    }

    pub async fn get_active_info(&self) -> Option<ActiveSyncInfo> {
        self.active_info.lock().await.clone()
    }

    pub async fn cancel(&self) -> bool {
        let mut guard = self.active_cancel.lock().await;
        if let Some((_, token)) = guard.take() {
            token.store(true, std::sync::atomic::Ordering::Relaxed);
            *self.active_info.lock().await = None;
            true
        } else {
            false
        }
    }

    pub async fn run_sync(
        self: Arc<Self>,
        module: WebsiteModule,
        engine: Arc<Mutex<LuaModuleEngine>>,
        db: Database,
        ws: broadcast::Sender<WsNotification>,
        catalog_ws: Option<broadcast::Sender<WsNotification>>,
    ) {
        let cancel_token = Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let mut guard = self.active_cancel.lock().await;
            if guard.is_some() {
                warn!(
                    "A catalog sync is already running; skipping {}",
                    module.name
                );
                return;
            }
            *guard = Some((module.id.clone(), cancel_token.clone()));
            let mut info_guard = self.active_info.lock().await;
            *info_guard = Some(ActiveSyncInfo {
                module_id: module.id.clone(),
                module_name: module.name.clone(),
                current_directory: 0,
                total_directories: module.total_directory.max(1),
                current_page: 0,
                total_pages: None,
                items_found: 0,
                started_at_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            });
        }

        let _ = db
            .set_module_sync_status(&module.id, "Syncing", None, None)
            .await;
        send_catalog_notification(
            &ws,
            catalog_ws.as_ref(),
            WsNotification {
                event: "catalog_sync_started".into(),
                payload: serde_json::json!({ "module_id": &module.id, "module_name": &module.name }),
            },
        );

        let file_path = module.file_path.clone();
        let cancel_clone = cancel_token.clone();
        let engine_clone = engine.clone();
        let db_clone = db.clone();
        let ws_clone = ws.clone();
        let catalog_ws_clone = catalog_ws.clone();
        let mod_id = module.id.clone();
        let mod_name = module.name.clone();
        let sm_clone = self.clone();

        let handle = tokio::runtime::Handle::current();
        let sync_result = tokio::task::spawn_blocking(move || {
            let eng = handle.block_on(async { engine_clone.lock().await });
            eng.update_catalog(&file_path, cancel_clone, |entries, progress| {
                let items: Vec<(String, String)> =
                    entries.into_iter().map(|e| (e.title, e.link)).collect();

                let db_c = db_clone.clone();
                let ws_c = ws_clone.clone();
                let catalog_ws_c = catalog_ws_clone.clone();
                let sm_c = sm_clone.clone();
                let m_id = mod_id.clone();
                let m_name = mod_name.clone();

                handle.block_on(async {
                    let _ = db_c.upsert_catalog_items(&m_id, &items).await;
                    {
                        let mut info_g = sm_c.active_info.lock().await;
                        if let Some(ref mut inf) = *info_g {
                            inf.current_directory = progress.directory_index;
                            inf.current_page = progress.current_page;
                            inf.total_pages = progress.total_pages;
                            inf.items_found = progress.items_found;
                        }
                    }
                    send_catalog_notification(
                        &ws_c,
                        catalog_ws_c.as_ref(),
                        WsNotification {
                            event: "catalog_sync_progress".into(),
                            payload: serde_json::json!({
                                "module_id": m_id,
                                "module_name": m_name,
                                "directory_index": progress.directory_index,
                                "total_directories": progress.total_directories,
                                "current_page": progress.current_page,
                                "total_pages": progress.total_pages,
                                "items_found": progress.items_found,
                            }),
                        },
                    );
                });

                Ok(())
            })
        })
        .await;

        match sync_result {
            Ok(Ok(total)) => {
                let _ = db
                    .set_module_sync_status(&module.id, "Completed", None, Some(total as i64))
                    .await;
                send_catalog_notification(
                    &ws,
                    catalog_ws.as_ref(),
                    WsNotification {
                        event: "catalog_sync_completed".into(),
                        payload: serde_json::json!({ "module_id": &module.id, "total_mangas": total }),
                    },
                );
            }
            Ok(Err(e)) => {
                let is_cancelled = cancel_token.load(std::sync::atomic::Ordering::Relaxed);
                let status = if is_cancelled { "Cancelled" } else { "Failed" };
                let err_msg = e.to_string();
                let _ = db
                    .set_module_sync_status(&module.id, status, Some(&err_msg), None)
                    .await;
                send_catalog_notification(
                    &ws,
                    catalog_ws.as_ref(),
                    WsNotification {
                        event: "catalog_sync_failed".into(),
                        payload: serde_json::json!({ "module_id": &module.id, "error": err_msg }),
                    },
                );
            }
            Err(e) => {
                let err_msg = format!("Task error: {}", e);
                let _ = db
                    .set_module_sync_status(&module.id, "Failed", Some(&err_msg), None)
                    .await;
                send_catalog_notification(
                    &ws,
                    catalog_ws.as_ref(),
                    WsNotification {
                        event: "catalog_sync_failed".into(),
                        payload: serde_json::json!({ "module_id": &module.id, "error": err_msg }),
                    },
                );
            }
        }

        {
            let mut guard = self.active_cancel.lock().await;
            *guard = None;
            let mut info_guard = self.active_info.lock().await;
            *info_guard = None;
        }
    }
}

async fn handle_download_failure(
    dl_state: &AppState,
    dl_id: &str,
    manga_title: &str,
    chapter_title: &str,
    retry_count: i64,
    max_retries: i64,
    err_msg: &str,
) {
    if retry_count < max_retries {
        let next_retry = retry_count + 1;
        let retry_note = format!(
            "Tentative {}/{} échouée ({})",
            next_retry, max_retries, err_msg
        );
        warn!(
            "Download worker: [{}] '{}' - '{}' failed (attempt {}/{}): {}. Scheduling auto-retry in queue...",
            dl_id, manga_title, chapter_title, next_retry, max_retries, err_msg
        );

        let _ = sqlx::query(
            "UPDATE downloads SET status = 'Pending', retry_count = ?1, error_message = ?2, created_at = CURRENT_TIMESTAMP WHERE id = ?3"
        )
        .bind(next_retry)
        .bind(&retry_note)
        .bind(dl_id)
        .execute(dl_state.db.pool())
        .await;

        let _ = dl_state.ws_sender.send(WsNotification {
            event: "download_progress".into(),
            payload: serde_json::json!({
                "id": dl_id,
                "status": "Pending",
                "retry_count": next_retry,
                "max_retries": max_retries,
                "error_message": retry_note
            }),
        });

        tokio::time::sleep(Duration::from_millis(1500)).await;
    } else {
        error!(
            "Download worker: [{}] '{}' - '{}' permanently failed after {} attempts: {}",
            dl_id, manga_title, chapter_title, max_retries, err_msg
        );

        let _ =
            sqlx::query("UPDATE downloads SET status = 'Failed', error_message = ?1 WHERE id = ?2")
                .bind(err_msg)
                .bind(dl_id)
                .execute(dl_state.db.pool())
                .await;

        let _ = dl_state.ws_sender.send(WsNotification {
            event: "download_failed".into(),
            payload: serde_json::json!({
                "id": dl_id,
                "status": "Failed",
                "error_message": err_msg,
                "retry_count": retry_count,
                "max_retries": max_retries
            }),
        });
    }
}

pub async fn run_server(addr: SocketAddr) -> anyhow::Result<()> {
    let base_path = base_path::from_env()?;
    if let Some(config) = demo::DemoConfig::from_env()? {
        return demo::run_server(addr, config, &base_path).await;
    }
    let db = Database::new("data/fmd.db").await?;

    // Auto-bootstrap admin user if credentials provided via env or k8s secret
    if let Err(e) = auth::bootstrap_admin_from_env_or_secret(&db).await {
        warn!("Admin bootstrap notice: {}", e);
    }

    let engine_raw = LuaModuleEngine::new().map_err(|e| anyhow::anyhow!("{}", e))?;

    // Scan modules if directory exists
    let modules = if Path::new("lua/modules").exists() {
        engine_raw
            .scan_modules_dir("lua/modules")
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let engine = Arc::new(Mutex::new(engine_raw));
    let (ws_sender, _) = broadcast::channel(100);
    let sync_manager = Arc::new(CatalogSyncManager::new());
    let throttler = auth::DomainThrottler::new();

    let nats_context = if let Ok(nats_url) = std::env::var("FMD_NATS_URL") {
        info!("Connecting to NATS JetStream at {}", nats_url);
        match async_nats::connect(&nats_url).await {
            Ok(client) => {
                let js = async_nats::jetstream::new(client);
                let stream_cfg = async_nats::jetstream::stream::Config {
                    name: fmd_core::STREAM_NAME.to_string(),
                    subjects: vec![fmd_core::SUBJECT_TASK_ALL.to_string()],
                    retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
                    ..Default::default()
                };
                match js.get_or_create_stream(stream_cfg).await {
                    Ok(_) => {
                        info!("NATS JetStream stream '{}' is ready", fmd_core::STREAM_NAME);
                        Some(js)
                    }
                    Err(e) => {
                        warn!("Could not create JetStream stream ({}): running in local standalone mode", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Could not connect to NATS at {} ({}): running in local standalone mode",
                    nats_url, e
                );
                None
            }
        }
    } else {
        None
    };

    let login_rate_limiter = Arc::new(auth::LoginRateLimiter::new());
    let internal_worker_token = match std::env::var("FMD_INTERNAL_TOKEN") {
        Ok(val) => {
            let trimmed = val.trim().to_string();
            if trimmed.is_empty() {
                anyhow::bail!("FMD_INTERNAL_TOKEN cannot be empty when set");
            }
            trimmed
        }
        Err(_) => {
            if std::env::var("FMD_NATS_URL")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            {
                warn!(
                    "FMD_NATS_URL is set without FMD_INTERNAL_TOKEN; generating ephemeral server token so external workers cannot authenticate without an explicit shared secret."
                );
            }
            fmd_core::auth::generate_secure_token()
        }
    };

    let state = AppState {
        db,
        shared_cache: None,
        demo_shutdown: None,
        engine,
        modules: Arc::new(modules),
        ws_sender: ws_sender.clone(),
        catalog_ws_sender: None,
        sync_manager,
        nats_context,
        throttler,
        login_rate_limiter,
        internal_worker_token,
    };

    // Spawn background task scheduler (weekly auto-resync and chapter checking)
    let bg_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
        let mut hour_ticks = 0u32;
        loop {
            interval.tick().await;
            hour_ticks = hour_ticks.wrapping_add(1);

            // 1. Hourly check on enabled favorites with auto_download
            if let Ok(favorites) = bg_state.db.list_favorites_with_stats().await {
                for fav in favorites {
                    if fav.enabled && fav.auto_download {
                        info!(
                            "Scheduler: checking auto-download updates for favorite '{}'...",
                            fav.title
                        );
                        let _ = check_favorite_internal(&bg_state, &fav).await;
                    }
                }
            }

            // 2. Weekly auto-resync check (every 6 hours)
            if hour_ticks.is_multiple_of(6) {
                info!("Background scheduler tick: checking modules for scheduled sync...");
                if !bg_state.sync_manager.is_syncing().await {
                    if let Ok(statuses) = bg_state.db.list_all_sync_statuses().await {
                        let now = chrono::Utc::now();
                        for st in statuses {
                            let should_sync = match st.last_synced_at {
                                Some(t) => (now - t).num_days() >= 7,
                                None => false,
                            };
                            if should_sync && st.status != "Syncing" {
                                if let Some(m) = bg_state
                                    .modules
                                    .iter()
                                    .find(|m| m.id == st.module_id)
                                    .cloned()
                                {
                                    info!("Triggering weekly auto-resync for module {}", m.name);
                                    bg_state
                                        .sync_manager
                                        .clone()
                                        .run_sync(
                                            m,
                                            bg_state.engine.clone(),
                                            bg_state.db.clone(),
                                            bg_state.ws_sender.clone(),
                                            bg_state.catalog_ws_sender.clone(),
                                        )
                                        .await;
                                    break; // Only one sync at a time
                                }
                            }
                        }
                    }
                }
            }
        }
    });

    // Spawn background download queue worker (standalone mode only)
    if state.nats_context.is_some() {
        info!(
            "NATS JetStream is active: background downloads are delegated to external worker pods"
        );
    } else {
        let dl_state = state.clone();
        tokio::spawn(async move {
            loop {
                let pending_opt = sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64)>(
                "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE status = 'Pending' ORDER BY created_at ASC LIMIT 1"
            )
            .fetch_optional(dl_state.db.pool())
            .await
            .ok()
            .flatten();

                if let Some((
                    dl_id,
                    manga_title,
                    chapter_title,
                    chapter_url,
                    module_id,
                    format_str,
                    retry_count,
                    max_retries,
                )) = pending_opt
                {
                    info!(
                    "Download worker: starting [{}] '{}' - '{}' [module: {}, format: {}, attempt: {}/{}]",
                    dl_id, manga_title, chapter_title, module_id, format_str, retry_count + 1, max_retries
                );

                    let _ =
                        sqlx::query("UPDATE downloads SET status = 'Downloading' WHERE id = ?1")
                            .bind(&dl_id)
                            .execute(dl_state.db.pool())
                            .await;

                    let _ = dl_state.ws_sender.send(WsNotification {
                        event: "download_progress".into(),
                        payload: serde_json::json!({
                            "id": &dl_id,
                            "status": "Downloading",
                            "downloaded_pages": 0,
                            "retry_count": retry_count,
                            "max_retries": max_retries,
                        }),
                    });

                    let module = dl_state
                        .modules
                        .iter()
                        .find(|m| m.id == module_id || m.name.eq_ignore_ascii_case(&module_id))
                        .cloned();

                    if let Some(m) = module {
                        let file_path = m.file_path.clone();
                        let url = chapter_url.clone();
                        let pages_res = {
                            let engine = dl_state.engine.lock().await;
                            engine.get_chapter_pages(&file_path, &url)
                        };

                        let pages = match pages_res {
                            Ok(p) if !p.is_empty() => p,
                            Ok(_) => {
                                let err = "Aucune page trouvée pour ce chapitre".to_string();
                                handle_download_failure(
                                    &dl_state,
                                    &dl_id,
                                    &manga_title,
                                    &chapter_title,
                                    retry_count,
                                    max_retries,
                                    &err,
                                )
                                .await;
                                continue;
                            }
                            Err(e) => {
                                let err =
                                    format!("Erreur lors de la récupération des pages: {}", e);
                                handle_download_failure(
                                    &dl_state,
                                    &dl_id,
                                    &manga_title,
                                    &chapter_title,
                                    retry_count,
                                    max_retries,
                                    &err,
                                )
                                .await;
                                continue;
                            }
                        };

                        let total = pages.len();
                        let _ = sqlx::query("UPDATE downloads SET total_pages = ?1 WHERE id = ?2")
                            .bind(total as i64)
                            .bind(&dl_id)
                            .execute(dl_state.db.pool())
                            .await;

                        info!(
                            "Download worker: [{}] '{}' - '{}': {} pages to download",
                            dl_id, manga_title, chapter_title, total
                        );

                        let mut packer = fmd_core::ChapterPacker::new(
                            manga_title.clone(),
                            chapter_title.clone(),
                        );
                        let mut download_err = None;

                        let http = match fmd_net::HttpClient::new() {
                            Ok(h) => h,
                            Err(e) => {
                                let err = format!("Erreur création HttpClient: {}", e);
                                handle_download_failure(
                                    &dl_state,
                                    &dl_id,
                                    &manga_title,
                                    &chapter_title,
                                    retry_count,
                                    max_retries,
                                    &err,
                                )
                                .await;
                                continue;
                            }
                        };

                        for (idx, page_url) in pages.iter().enumerate() {
                            const MAX_PAGE_RETRIES: usize = 3;
                            let mut page_success = false;
                            let mut last_page_err = String::new();

                            for attempt in 1..=MAX_PAGE_RETRIES {
                                let (custom_headers, custom_bytes) = {
                                    let engine = dl_state.engine.lock().await;
                                    let headers = engine
                                        .prepare_download_image(&file_path, page_url, idx)
                                        .unwrap_or_default();
                                    let maybe_bytes = engine
                                        .download_custom_image(&file_path, page_url, idx)
                                        .ok()
                                        .flatten();
                                    (headers, maybe_bytes)
                                };

                                let page_res = match custom_bytes {
                                    Some(bytes) => Ok(bytes),
                                    None => {
                                        http.get_bytes_with_headers(page_url, &custom_headers).await
                                    }
                                };

                                match page_res {
                                    Ok(bytes) if !bytes.is_empty() => {
                                        let ext = if page_url.contains(".webp") {
                                            "webp"
                                        } else if page_url.contains(".png") {
                                            "png"
                                        } else {
                                            "jpg"
                                        };
                                        let page_name = format!("{:03}.{}", idx + 1, ext);
                                        packer.add_page(page_name, bytes);

                                        let _ = sqlx::query("UPDATE downloads SET downloaded_pages = ?1 WHERE id = ?2")
                                        .bind((idx + 1) as i64)
                                        .bind(&dl_id)
                                        .execute(dl_state.db.pool())
                                        .await;

                                        let _ = dl_state.ws_sender.send(WsNotification {
                                            event: "download_progress".into(),
                                            payload: serde_json::json!({
                                                "id": &dl_id,
                                                "status": "Downloading",
                                                "downloaded_pages": idx + 1,
                                                "total_pages": total,
                                                "retry_count": retry_count,
                                                "max_retries": max_retries,
                                            }),
                                        });

                                        if attempt > 1 {
                                            info!(
                                            "Download worker: [{}] page {}/{} recovered on attempt {}/{}",
                                            dl_id, idx + 1, total, attempt, MAX_PAGE_RETRIES
                                        );
                                        }

                                        page_success = true;
                                        break;
                                    }
                                    Ok(_) => {
                                        last_page_err = format!("Page {} retournée vide", idx + 1);
                                        warn!(
                                        "Download worker: [{}] page {}/{} attempt {}/{} failed: {}",
                                        dl_id, idx + 1, total, attempt, MAX_PAGE_RETRIES, last_page_err
                                    );
                                    }
                                    Err(e) => {
                                        last_page_err = format!("Page {}: {}", idx + 1, e);
                                        warn!(
                                        "Download worker: [{}] page {}/{} attempt {}/{} failed: {}",
                                        dl_id, idx + 1, total, attempt, MAX_PAGE_RETRIES, e
                                    );
                                    }
                                }

                                if attempt < MAX_PAGE_RETRIES {
                                    tokio::time::sleep(Duration::from_millis(
                                        1000 * attempt as u64,
                                    ))
                                    .await;
                                }
                            }

                            if !page_success {
                                download_err = Some(last_page_err);
                                break;
                            }
                        }

                        if let Some(err) = download_err {
                            handle_download_failure(
                                &dl_state,
                                &dl_id,
                                &manga_title,
                                &chapter_title,
                                retry_count,
                                max_retries,
                                &err,
                            )
                            .await;
                        } else {
                            let settings = dl_state.db.get_settings().await.unwrap_or_default();
                            let chap_num = fmd_core::extract_chapter_number(&chapter_title)
                                .unwrap_or_default();

                            let prefix_id = format!("{}:%", module_id);
                            let manga_meta = sqlx::query_as::<
                                _,
                                (
                                    Option<String>,
                                    Option<String>,
                                    Option<String>,
                                    Option<String>,
                                ),
                            >(
                                r#"
                            SELECT author, genres, description, original_url
                            FROM mangas
                            WHERE title = ?1 OR id LIKE ?2
                            ORDER BY (author IS NOT NULL AND author != '') DESC, updated_at DESC
                            LIMIT 1
                            "#,
                            )
                            .bind(&manga_title)
                            .bind(&prefix_id)
                            .fetch_optional(dl_state.db.pool())
                            .await
                            .ok()
                            .flatten();

                            let genres_vec = manga_meta
                                .as_ref()
                                .and_then(|m| m.1.as_ref())
                                .map(|g| {
                                    if let Ok(vec) = serde_json::from_str::<Vec<String>>(g) {
                                        vec
                                    } else {
                                        g.split(',')
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect()
                                    }
                                })
                                .unwrap_or_default();

                            let lang_iso = match m.category.to_lowercase().as_str() {
                                "french" | "fr" => "fr",
                                "english" | "en" => "en",
                                "spanish" | "es" => "es",
                                "japanese" | "ja" => "ja",
                                _ => "en",
                            };

                            packer.set_metadata(fmd_core::ChapterMetadata {
                                series: manga_title.clone(),
                                title: chapter_title.clone(),
                                number: Some(chap_num.clone()),
                                summary: manga_meta.as_ref().and_then(|m| m.2.clone()),
                                writer: manga_meta.as_ref().and_then(|m| m.0.clone()),
                                genres: genres_vec,
                                web: manga_meta.as_ref().and_then(|m| m.3.clone()),
                                language_iso: Some(lang_iso.to_string()),
                            });

                            let (fmt, ext) = match format_str.to_lowercase().as_str() {
                                "cbz" | "cbr" => (fmd_core::PackagingFormat::Cbz, "cbz"),
                                "pdf" => (fmd_core::PackagingFormat::Pdf, "pdf"),
                                "epub" => (fmd_core::PackagingFormat::Epub, "epub"),
                                _ => (fmd_core::PackagingFormat::Directory, ""),
                            };

                            let base_dir = std::path::Path::new(&settings.download_dir);
                            let out_path = fmd_core::format_download_path(
                                base_dir,
                                &settings.naming_template,
                                &manga_title,
                                &chapter_title,
                                &chap_num,
                                &module_id,
                                ext,
                            );

                            info!(
                                "Download worker: [{}] packaging '{}' - '{}' ({} pages) to {}...",
                                dl_id, manga_title, chapter_title, total, format_str
                            );

                            match packer.pack_to(fmt, &out_path) {
                                Ok(_) => {
                                    let _ = sqlx::query("UPDATE downloads SET status = 'Completed', downloaded_pages = total_pages, error_message = NULL WHERE id = ?1")
                                    .bind(&dl_id)
                                    .execute(dl_state.db.pool())
                                    .await;

                                    let _ =
                                        dl_state.db.mark_chapter_downloaded("", &chapter_url).await;

                                    info!(
                                    "Download worker: [{}] successfully completed '{}' - '{}' -> {}",
                                    dl_id, manga_title, chapter_title, out_path.display()
                                );

                                    let _ = dl_state.ws_sender.send(WsNotification {
                                    event: "download_completed".into(),
                                    payload: serde_json::json!({ "id": &dl_id, "status": "Completed" }),
                                });
                                }
                                Err(e) => {
                                    let err = format!("Erreur lors de l'archivage: {}", e);
                                    handle_download_failure(
                                        &dl_state,
                                        &dl_id,
                                        &manga_title,
                                        &chapter_title,
                                        retry_count,
                                        max_retries,
                                        &err,
                                    )
                                    .await;
                                }
                            }
                        }
                    } else {
                        let err = format!("Module '{}' introuvable", module_id);
                        error!("Download worker: [{}] {}", dl_id, err);
                        let _ = sqlx::query("UPDATE downloads SET status = 'Failed', error_message = ?1 WHERE id = ?2")
                        .bind(&err)
                        .bind(&dl_id)
                        .execute(dl_state.db.pool())
                        .await;

                        let _ = dl_state.ws_sender.send(WsNotification {
                        event: "download_failed".into(),
                        payload: serde_json::json!({ "id": &dl_id, "status": "Failed", "error_message": &err }),
                    });
                    }
                } else {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                }
            }
        });
    }

    // Startup reconciliation: fix any chapters erroneously flagged as downloaded without a completed download record
    if let Ok(cleaned) = state.db.reconcile_downloaded_chapters().await {
        if cleaned > 0 {
            info!(
                "Reconciled {} erroneously marked downloaded chapters in database",
                cleaned
            );
        }
    }

    let app = with_base_path(build_router(state, &base_path), &base_path);
    info!("Starting FMD3 San (三) Daemon on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: AppState, base_path: &str) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::PATCH,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::ACCEPT,
            axum::http::header::CONTENT_TYPE,
            axum::http::header::COOKIE,
            axum::http::HeaderName::from_static("x-fmd-internal-token"),
        ]);

    let index_html = Arc::<str>::from(ui::render_index_html(base_path));
    let root_index_html = index_html.clone();
    let fallback_index_html = index_html.clone();
    let app = Router::new()
        .route(
            "/",
            get(move || {
                let html = root_index_html.clone();
                async move { Html(html.to_string()) }
            }),
        )
        .route(
            "/logo.png",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/png"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    LOGO_PNG,
                )
            }),
        )
        .route(
            "/favicon.ico",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/x-icon"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    FAVICON_ICO,
                )
            }),
        )
        .route(
            "/favicon-32x32.png",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/png"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    FAVICON_32,
                )
            }),
        )
        .route(
            "/favicon-16x16.png",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/png"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    FAVICON_16,
                )
            }),
        )
        .route(
            "/apple-touch-icon.png",
            get(|| async {
                (
                    [
                        (axum::http::header::CONTENT_TYPE, "image/png"),
                        (axum::http::header::CACHE_CONTROL, "public, max-age=86400"),
                    ],
                    APPLE_TOUCH_ICON,
                )
            }),
        )
        .route("/api/health", get(|| async { "OK" }))
        .route("/api/version", get(|| async { env!("CARGO_PKG_VERSION") }))
        .route(
            "/api/demo/info",
            get(|| async { Json(serde_json::json!({"demo": false})) }),
        )
        .route("/api/auth/status", get(auth::get_auth_status))
        .route("/api/auth/setup", post(auth::auth_setup))
        .route("/api/auth/login", post(auth::auth_login))
        .route("/api/auth/logout", post(auth::auth_logout))
        .route("/api/auth/me", get(auth::auth_me))
        .route(
            "/api/auth/tokens",
            get(auth::list_api_tokens).post(auth::create_api_token_handler),
        )
        .route(
            "/api/auth/tokens/{id}",
            delete(auth::delete_api_token_handler),
        )
        .route(
            "/api/admin/users",
            get(auth::admin_list_users).post(auth::admin_create_user),
        )
        .route(
            "/api/admin/users/{id}",
            put(auth::admin_update_user).delete(auth::admin_delete_user),
        )
        .route(
            "/api/admin/cache/policies",
            get(auth::admin_list_cache_policies).post(auth::admin_set_cache_policy),
        )
        .route(
            "/api/admin/cache/{manga_id}",
            delete(auth::admin_clear_manga_cache),
        )
        .route("/api/modules", get(list_modules))
        .route("/api/mangas", get(list_mangas))
        .route("/api/manga/fetch", post(fetch_manga_info))
        .route("/api/manga/alternatives", get(get_alternative_sources))
        .route("/api/manga/merge", post(merge_manga_sources))
        .route("/api/favorites", get(list_favorites).post(add_favorite))
        .route("/api/favorites/check-all", post(check_all_favorites))
        .route(
            "/api/favorites/{id}",
            delete(remove_favorite).patch(update_favorite_settings),
        )
        .route("/api/favorites/{id}/check", post(check_single_favorite))
        .route("/api/favorites/{id}/chapters", get(list_favorite_chapters))
        .route("/api/downloads", get(list_downloads))
        .route("/api/downloads/queue", post(queue_downloads))
        .route(
            "/api/downloads/retry-failed",
            post(retry_all_failed_downloads),
        )
        .route("/api/downloads/{id}", delete(remove_download))
        .route("/api/downloads/{id}/retry", post(retry_download))
        .route("/api/catalog/search", get(search_catalog))
        .route("/api/catalog/status", get(get_catalog_status))
        .route("/api/catalog/sync/{module_id}", post(trigger_catalog_sync))
        .route("/api/catalog/sync/cancel", post(cancel_catalog_sync))
        .route("/api/proxy/image", get(proxy_image))
        .route(
            "/api/settings",
            get(get_settings_handler).post(update_settings_handler),
        )
        .route("/api/internal/tasks/progress", post(internal_task_progress))
        .route("/api/internal/tasks/complete", post(internal_task_complete))
        .route("/api/ws", get(ws_handler))
        .fallback(get(move || {
            let html = fallback_index_html.clone();
            async move { Html(html.to_string()) }
        }))
        .with_state(state);
    app.layer(axum::middleware::from_fn(add_security_headers))
        .layer(cors)
}

fn with_base_path(app: Router, base_path: &str) -> Router {
    if base_path.is_empty() {
        app
    } else {
        Router::new()
            .fallback_service(app)
            .layer(axum::middleware::from_fn_with_state(
                base_path.to_owned(),
                strip_base_path,
            ))
    }
}

async fn strip_base_path(
    State(base_path): State<String>,
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let uri = req.uri();
    if let Some(rest) = uri.path().strip_prefix(&base_path) {
        if rest.is_empty() || rest.starts_with('/') {
            let path = if rest.is_empty() { "/" } else { rest };
            let path_and_query = match uri.query() {
                Some(query) => format!("{path}?{query}"),
                None => path.to_owned(),
            };
            let mut parts = uri.clone().into_parts();
            parts.path_and_query = match path_and_query.parse() {
                Ok(path_and_query) => Some(path_and_query),
                Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
            };
            *req.uri_mut() = match axum::http::Uri::from_parts(parts) {
                Ok(uri) => uri,
                Err(_) => return axum::http::StatusCode::BAD_REQUEST.into_response(),
            };
        }
    }
    next.run(req).await
}

async fn add_security_headers(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        axum::http::HeaderName::from_static("x-xss-protection"),
        axum::http::HeaderValue::from_static("1; mode=block"),
    );
    response
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn verify_worker_token(
    headers: &axum::http::HeaderMap,
    expected_token: &str,
) -> Result<(), (axum::http::StatusCode, String)> {
    if expected_token.is_empty() {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Accès refusé : jeton interne worker non configuré".into(),
        ));
    }
    let token = headers
        .get("x-fmd-internal-token")
        .and_then(|h| h.to_str().ok())
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|h| h.strip_prefix("Bearer "))
        });

    if let Some(t) = token {
        if constant_time_eq(t.trim().as_bytes(), expected_token.as_bytes()) {
            return Ok(());
        }
    }

    Err((
        axum::http::StatusCode::FORBIDDEN,
        "Accès refusé : jeton interne worker manquant ou invalide".into(),
    ))
}

async fn list_modules(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
) -> Json<Vec<WebsiteModule>> {
    Json((*state.modules).clone())
}

#[derive(Serialize)]
struct MangaRow {
    id: String,
    module_id: String,
    title: String,
    original_url: String,
    status: String,
}

async fn list_mangas(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
) -> Json<Vec<MangaRow>> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT id, module_id, title, original_url, status FROM mangas LIMIT 100",
    )
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let result = rows
        .into_iter()
        .map(|(id, module_id, title, original_url, status)| MangaRow {
            id,
            module_id,
            title,
            original_url,
            status,
        })
        .collect();

    Json(result)
}

#[derive(Deserialize)]
struct FetchMangaRequest {
    module_id: String,
    url: String,
    #[serde(default)]
    force_refresh: bool,
}

async fn save_fetched_manga_metadata(
    db: &Database,
    manga_id: &str,
    req: &FetchMangaRequest,
    info: &MangaInfoResult,
) -> Result<(), (axum::http::StatusCode, String)> {
    db.upsert_manga_metadata(
        manga_id,
        &req.module_id,
        &info.title,
        &req.url,
        info.cover_url.as_deref(),
        info.authors.as_deref(),
        info.summary.as_deref(),
        Some(&info.genres),
    )
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn fetch_manga_info(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    Json(req): Json<FetchMangaRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let allow_force = req.force_refresh && ctx.permissions.can_force_refresh;
    load_manga_info(&state, &req, allow_force).await
}

async fn load_manga_info(
    state: &AppState,
    req: &FetchMangaRequest,
    allow_force: bool,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let manga_id = format!("{}:{}", req.module_id, req.url);
    if state.shared_cache.is_some()
        && (req.url.contains("://") || req.url.starts_with("//"))
        && !fmd_net::is_public_url(&req.url)
    {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Forbidden destination".into(),
        ));
    }

    // 1. Check cache if not forcing refresh
    if !allow_force {
        if let Ok(Some(cached)) = state.cache_db().get_manga_cache(&manga_id).await {
            if !cached.is_expired()
                && (state.shared_cache.is_none()
                    || chrono::Utc::now() < cached.cached_at + chrono::Duration::hours(24))
            {
                if let (Ok(mut info), Ok(chapters)) = (
                    serde_json::from_str::<MangaInfoResult>(&cached.metadata_json),
                    serde_json::from_str::<Vec<fmd_engine_lua::ChapterItem>>(&cached.chapters_json),
                ) {
                    info.chapters = chapters;
                    save_fetched_manga_metadata(&state.db, &manga_id, req, &info).await?;
                    let mut val = serde_json::to_value(&info).unwrap_or_default();
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert("is_cached".to_string(), serde_json::Value::Bool(true));
                        obj.insert(
                            "cached_at".to_string(),
                            serde_json::to_value(cached.cached_at)
                                .unwrap_or(serde_json::Value::Null),
                        );
                        obj.insert(
                            "expires_at".to_string(),
                            serde_json::to_value(cached.expires_at)
                                .unwrap_or(serde_json::Value::Null),
                        );
                    }
                    return Ok(Json(val));
                }
            }
        }
    }

    // 2. Fetch fresh from engine with rate limiting
    let policy = state
        .cache_db()
        .get_module_cache_policy(&req.module_id)
        .await
        .unwrap_or_default();

    state
        .throttler
        .throttle(&req.module_id, policy.min_delay_ms)
        .await;

    let module = state
        .modules
        .iter()
        .find(|m| m.id == req.module_id || m.name.eq_ignore_ascii_case(&req.module_id))
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Module {} introuvable", req.module_id),
            )
        })?;

    let engine = state.engine.lock().await;
    match engine.get_manga_info(&module.file_path, &req.url) {
        Ok(info) => {
            save_fetched_manga_metadata(&state.db, &manga_id, req, &info).await?;

            let now = chrono::Utc::now();
            let expires_at = if state.shared_cache.is_some() {
                now + chrono::Duration::hours(24)
            } else {
                policy.calculate_expiration(now)
            };
            let metadata_json = serde_json::to_string(&info).unwrap_or_default();
            let chapters_json = serde_json::to_string(&info.chapters).unwrap_or_default();

            let cache_item = fmd_core::MangaCacheItem {
                manga_id,
                module_id: req.module_id.clone(),
                title: info.title.clone(),
                cover_url: info.cover_url.clone(),
                metadata_json,
                chapters_json,
                cached_at: now,
                expires_at,
            };
            if let Err(error) = state.cache_db().save_manga_cache(&cache_item).await {
                warn!("Failed to cache manga {}: {error}", cache_item.manga_id);
            }

            let mut val = serde_json::to_value(&info).unwrap_or_default();
            if let Some(obj) = val.as_object_mut() {
                obj.insert("is_cached".to_string(), serde_json::Value::Bool(false));
                obj.insert(
                    "cached_at".to_string(),
                    serde_json::to_value(now).unwrap_or(serde_json::Value::Null),
                );
                obj.insert(
                    "expires_at".to_string(),
                    serde_json::to_value(expires_at).unwrap_or(serde_json::Value::Null),
                );
            }
            Ok(Json(val))
        }
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erreur lors de l'extraction: {}", e),
        )),
    }
}

async fn load_demo_manga_info(
    state: &AppState,
    module_id: &str,
    url: &str,
) -> Result<MangaInfoResult, (axum::http::StatusCode, String)> {
    let req = FetchMangaRequest {
        module_id: module_id.to_owned(),
        url: url.to_owned(),
        force_refresh: false,
    };
    let Json(value) = load_manga_info(state, &req, false).await?;
    serde_json::from_value(value)
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

#[derive(Deserialize)]
pub struct AlternativesQuery {
    pub q: String,
    pub exclude_module: Option<String>,
}

#[derive(Serialize)]
pub struct AlternativeSourceItem {
    pub module_id: String,
    pub module_name: String,
    pub title: String,
    pub link: String,
    pub cover_url: Option<String>,
}

async fn get_alternative_sources(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
    Query(params): Query<AlternativesQuery>,
) -> Json<Vec<AlternativeSourceItem>> {
    let q = params.q.trim();
    if q.is_empty() {
        return Json(Vec::new());
    }

    let (items, _) = state
        .cache_db()
        .search_catalog(q, None, 50, 0)
        .await
        .unwrap_or_default();
    let exclude = params.exclude_module.unwrap_or_default();

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for item in items {
        if !exclude.is_empty()
            && (item.module_id == exclude || item.module_id.eq_ignore_ascii_case(&exclude))
        {
            continue;
        }

        let key = format!("{}:{}", item.module_id, item.link);
        if seen.contains(&key) {
            continue;
        }
        seen.insert(key);

        let module_name = state
            .modules
            .iter()
            .find(|m| m.id == item.module_id || m.name.eq_ignore_ascii_case(&item.module_id))
            .map(|m| m.name.clone())
            .unwrap_or_else(|| item.module_id.clone());

        result.push(AlternativeSourceItem {
            module_id: item.module_id,
            module_name,
            title: item.title,
            link: item.link,
            cover_url: item.cover_url,
        });
    }

    Json(result)
}

#[derive(Deserialize)]
pub struct MergeSourceSpec {
    pub module_id: String,
    pub url: String,
    #[serde(default)]
    pub chapters: Option<Vec<fmd_core::ChapterItem>>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum SecondarySources {
    Single(MergeSourceSpec),
    Multiple(Vec<MergeSourceSpec>),
}

#[derive(Deserialize)]
pub struct MergeMangaRequest {
    pub primary: MergeSourceSpec,
    pub secondary: SecondarySources,
}

async fn merge_manga_sources(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    Json(req): Json<MergeMangaRequest>,
) -> Result<Json<fmd_core::MergedMangaResult>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    let primary_module = state
        .modules
        .iter()
        .find(|m| {
            m.id == req.primary.module_id || m.name.eq_ignore_ascii_case(&req.primary.module_id)
        })
        .cloned()
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Module primaire '{}' introuvable", req.primary.module_id),
            )
        })?;

    let primary_chaps = match req.primary.chapters {
        Some(ch) if !ch.is_empty() => ch,
        _ => {
            let info = if state.shared_cache.is_some() {
                load_demo_manga_info(&state, &req.primary.module_id, &req.primary.url).await?
            } else {
                let engine = state.engine.lock().await;
                engine
                    .get_manga_info(&primary_module.file_path, &req.primary.url)
                    .map_err(|e| {
                        (
                            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Erreur source primaire: {}", e),
                        )
                    })?
            };
            info.chapters
                .into_iter()
                .map(|c| fmd_core::ChapterItem {
                    title: c.title,
                    url: c.url,
                })
                .collect()
        }
    };

    let secondaries = match req.secondary {
        SecondarySources::Single(s) => vec![s],
        SecondarySources::Multiple(m) => m,
    };

    let mut fallback_data: Vec<((String, String), Vec<fmd_core::ChapterItem>)> = Vec::new();

    for sec in secondaries {
        let sec_module = state
            .modules
            .iter()
            .find(|m| m.id == sec.module_id || m.name.eq_ignore_ascii_case(&sec.module_id))
            .cloned()
            .ok_or_else(|| {
                (
                    axum::http::StatusCode::NOT_FOUND,
                    format!("Module secondaire '{}' introuvable", sec.module_id),
                )
            })?;

        let sec_chaps = match sec.chapters {
            Some(ch) if !ch.is_empty() => ch,
            _ => {
                let info = if state.shared_cache.is_some() {
                    load_demo_manga_info(&state, &sec.module_id, &sec.url).await?
                } else {
                    let engine = state.engine.lock().await;
                    engine
                        .get_manga_info(&sec_module.file_path, &sec.url)
                        .map_err(|e| {
                            (
                                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                format!("Erreur source secondaire ({}): {}", sec_module.name, e),
                            )
                        })?
                };
                info.chapters
                    .into_iter()
                    .map(|c| fmd_core::ChapterItem {
                        title: c.title,
                        url: c.url,
                    })
                    .collect()
            }
        };

        fallback_data.push(((sec_module.id, sec_module.name), sec_chaps));
    }

    let fallback_slices: Vec<((&str, &str), &[fmd_core::ChapterItem])> = fallback_data
        .iter()
        .map(|((id, name), chaps)| ((id.as_str(), name.as_str()), chaps.as_slice()))
        .collect();

    let result = fmd_core::merge_chapter_sources(
        (&primary_module.id, &primary_module.name),
        &primary_chaps,
        &fallback_slices,
    );

    Ok(Json(result))
}

#[derive(Deserialize)]
struct AddFavoriteRequest {
    module_id: String,
    url: String,
    title: String,
    cover_url: Option<String>,
    author: Option<String>,
    genres: Option<Vec<String>>,
    description: Option<String>,
    auto_download: Option<bool>,
    download_format: Option<String>,
}

async fn add_favorite(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    Json(req): Json<AddFavoriteRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    let user_id = Some(ctx.user.id.as_str());
    let manga_id = format!("{}:{}", req.module_id, req.url);
    if state.shared_cache.is_some() && req.auto_download == Some(true) {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Automatic downloads are unavailable in demo mode".into(),
        ));
    }
    let auto_dl = req.auto_download.unwrap_or(false);
    let fmt = req.download_format.unwrap_or_else(|| "CBZ".into());

    state
        .db
        .add_or_update_favorite_with_meta_for_user(
            user_id,
            &manga_id,
            &req.module_id,
            &req.title,
            &req.url,
            req.cover_url.as_deref(),
            req.author.as_deref(),
            req.description.as_deref(),
            req.genres.as_deref(),
            auto_dl,
            &fmt,
        )
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Pre-fetch initial chapters and mark them as seen (not downloaded) so they aren't treated as new later
    if let Some(m) = state
        .modules
        .iter()
        .find(|m| m.id == req.module_id || m.name.eq_ignore_ascii_case(&req.module_id))
    {
        let file_path = m.file_path.clone();
        let url = req.url.clone();
        let engine = state.engine.clone();
        let db = state.db.clone();
        let m_id = manga_id.clone();
        let mod_id = req.module_id.clone();
        let demo_state = state.shared_cache.as_ref().map(|_| state.clone());
        tokio::spawn(async move {
            let chapters_res = if let Some(state) = demo_state {
                load_demo_manga_info(&state, &mod_id, &url)
                    .await
                    .map_err(|(_, error)| error)
            } else {
                let eng = engine.lock().await;
                eng.get_manga_info(&file_path, &url)
                    .map_err(|e| e.to_string())
            };
            match chapters_res {
                Ok(info) => {
                    if let Err(error) = db
                        .upsert_manga_metadata(
                            &m_id,
                            &mod_id,
                            &info.title,
                            &url,
                            info.cover_url.as_deref(),
                            info.authors.as_deref(),
                            info.summary.as_deref(),
                            Some(&info.genres),
                        )
                        .await
                    {
                        warn!("Failed to save favorite metadata for {m_id}: {error}");
                        return;
                    }
                    let chaps: Vec<fmd_core::ChapterItem> = info
                        .chapters
                        .into_iter()
                        .map(|c| fmd_core::ChapterItem {
                            title: c.title,
                            url: c.url,
                        })
                        .collect();
                    if let Err(error) = db.record_chapters(&m_id, &chaps, false).await {
                        warn!("Failed to record favorite chapters for {m_id}: {error}");
                    }
                }
                Err(error) => warn!("Failed to prefetch favorite chapters for {m_id}: {error}"),
            }
        });
    }

    // Broadcast WS event
    let _ = state.ws_sender.send(WsNotification {
        event: "favorite_added".into(),
        payload: serde_json::json!({
            "manga_id": manga_id,
            "title": req.title,
            "auto_download": auto_dl,
            "download_format": fmt,
        }),
    });

    Ok(Json(
        serde_json::json!({ "success": true, "manga_id": manga_id }),
    ))
}

async fn remove_favorite(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    sqlx::query("DELETE FROM favorites WHERE manga_id = ?1")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _ = state.ws_sender.send(WsNotification {
        event: "favorite_removed".into(),
        payload: serde_json::json!({ "manga_id": id }),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn list_favorites(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
) -> Json<Vec<fmd_core::Favorite>> {
    let favs = state
        .db
        .list_favorites_for_user(Some(&ctx.user.id))
        .await
        .unwrap_or_default();
    Json(favs)
}

#[derive(Deserialize)]
struct UpdateFavoriteSettingsRequest {
    enabled: Option<bool>,
    auto_download: Option<bool>,
    download_format: Option<String>,
}

async fn update_favorite_settings(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(req): Json<UpdateFavoriteSettingsRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    let current_favs = state
        .db
        .list_favorites_with_stats()
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let current = current_favs
        .into_iter()
        .find(|f| f.manga_id == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                "Favori introuvable".into(),
            )
        })?;

    let enabled = req.enabled.unwrap_or(current.enabled);
    if state.shared_cache.is_some() && req.auto_download == Some(true) {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Automatic downloads are unavailable in demo mode".into(),
        ));
    }
    let auto_download = req.auto_download.unwrap_or(current.auto_download);
    let download_format = req.download_format.unwrap_or(current.download_format);

    state
        .db
        .update_favorite_settings(&id, enabled, auto_download, &download_format)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _ = state.ws_sender.send(WsNotification {
        event: "favorite_settings_updated".into(),
        payload: serde_json::json!({
            "manga_id": id,
            "enabled": enabled,
            "auto_download": auto_download,
            "download_format": download_format,
        }),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn check_favorite_internal(
    state: &AppState,
    fav: &fmd_core::Favorite,
) -> Result<Vec<fmd_core::ChapterItem>, (axum::http::StatusCode, String)> {
    let module = state
        .modules
        .iter()
        .find(|m| m.id == fav.module_id || m.name.eq_ignore_ascii_case(&fav.module_id))
        .ok_or_else(|| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Module '{}' introuvable", fav.module_id),
            )
        })?;

    let file_path = module.file_path.clone();
    let url = fav.original_url.clone();

    let info = if state.shared_cache.is_some() {
        load_demo_manga_info(state, &fav.module_id, &url).await?
    } else {
        let engine = state.engine.lock().await;
        engine
            .get_manga_info(&file_path, &url)
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    };

    let _ = state
        .db
        .upsert_manga_metadata(
            &fav.manga_id,
            &fav.module_id,
            &fav.title,
            &fav.original_url,
            info.cover_url.as_deref(),
            info.authors.as_deref(),
            info.summary.as_deref(),
            Some(&info.genres),
        )
        .await;

    let chapters: Vec<fmd_core::ChapterItem> = info
        .chapters
        .into_iter()
        .map(|c| fmd_core::ChapterItem {
            title: c.title,
            url: c.url,
        })
        .collect();

    let new_chapters = state
        .db
        .check_favorite_new_chapters(&fav.manga_id, &chapters)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !new_chapters.is_empty() {
        info!(
            "Manga '{}': {} nouveaux chapitres détectés !",
            fav.title,
            new_chapters.len()
        );

        if fav.auto_download {
            for ch in &new_chapters {
                let dl_id = uuid::Uuid::new_v4().to_string();
                let _ = sqlx::query(
                    r#"
                    INSERT INTO downloads (id, manga_title, chapter_title, chapter_url, module_id, format, status)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Pending')
                    "#,
                )
                .bind(&dl_id)
                .bind(&fav.title)
                .bind(&ch.title)
                .bind(&ch.url)
                .bind(&fav.module_id)
                .bind(&fav.download_format)
                .execute(state.db.pool())
                .await;

                let _ = state.ws_sender.send(WsNotification {
                    event: "download_queued".into(),
                    payload: serde_json::json!({
                        "id": dl_id,
                        "manga_title": fav.title,
                        "chapter_title": ch.title,
                        "format": fav.download_format,
                    }),
                });
            }
        }

        let _ = state.ws_sender.send(WsNotification {
            event: "favorite_updated".into(),
            payload: serde_json::json!({
                "manga_id": fav.manga_id,
                "title": fav.title,
                "new_chapters": new_chapters.len(),
            }),
        });
    }

    Ok(new_chapters)
}

async fn check_single_favorite(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    let current_favs = state
        .db
        .list_favorites_with_stats()
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let fav = current_favs
        .into_iter()
        .find(|f| f.manga_id == id)
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                "Favori introuvable".into(),
            )
        })?;

    let new_chaps = check_favorite_internal(&state, &fav).await?;

    Ok(Json(serde_json::json!({
        "success": true,
        "manga_id": id,
        "new_chapters_count": new_chaps.len(),
    })))
}

async fn check_all_favorites(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_manage_favorites {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_manage_favorites' requise".into(),
        ));
    }
    let current_favs = state
        .db
        .list_favorites_with_stats()
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut checked_count = 0;
    let mut total_new = 0;

    for fav in current_favs {
        if fav.enabled {
            match check_favorite_internal(&state, &fav).await {
                Ok(new_chaps) => {
                    checked_count += 1;
                    total_new += new_chaps.len();
                }
                Err(error) if state.shared_cache.is_some() => return Err(error),
                Err((_, error)) => warn!("Failed to check favorite {}: {error}", fav.manga_id),
            }
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "checked_count": checked_count,
        "total_new_chapters": total_new,
    })))
}

#[derive(Serialize)]
struct ChapterStatusItem {
    title: String,
    url: String,
    is_downloaded: bool,
}

async fn list_favorite_chapters(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<Vec<ChapterStatusItem>>, (axum::http::StatusCode, String)> {
    let list = state
        .db
        .list_manga_chapters(&id)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let res = list
        .into_iter()
        .map(|(title, url, is_downloaded)| ChapterStatusItem {
            title,
            url,
            is_downloaded,
        })
        .collect();

    Ok(Json(res))
}

async fn ws_handler(
    _auth: auth::RequireAuth,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.ws_sender.subscribe();
    let mut catalog_rx = state
        .catalog_ws_sender
        .as_ref()
        .map(broadcast::Sender::subscribe);
    let mut shutdown = state.demo_shutdown.clone();

    // Welcome message
    let welcome = serde_json::json!({
        "event": "connected",
        "version": env!("CARGO_PKG_VERSION")
    });
    let _ = socket.send(Message::Text(welcome.to_string().into())).await;

    loop {
        tokio::select! {
            _ = async {
                match &mut shutdown {
                    Some(signal) => { let _ = signal.changed().await; }
                    None => std::future::pending::<()>().await,
                }
            } => {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        info!("Received WS text: {}", text);
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
            Ok(notif) = rx.recv() => {
                if let Ok(json_str) = serde_json::to_string(&notif) {
                    if socket.send(Message::Text(json_str.into())).await.is_err() {
                        break;
                    }
                }
            }
            Some(Ok(notif)) = async {
                match &mut catalog_rx {
                    Some(receiver) => Some(receiver.recv().await),
                    None => std::future::pending::<Option<Result<WsNotification, broadcast::error::RecvError>>>().await,
                }
            } => {
                if let Ok(json_str) = serde_json::to_string(&notif) {
                    if socket.send(Message::Text(json_str.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct QueueChapterItem {
    pub title: String,
    pub url: String,
    pub module_id: Option<String>,
}

#[derive(Deserialize)]
pub struct QueueDownloadRequest {
    pub module_id: String,
    pub manga_title: String,
    pub manga_url: Option<String>,
    pub author: Option<String>,
    pub genres: Option<Vec<String>>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub format: Option<String>,
    pub chapters: Vec<QueueChapterItem>,
}

#[derive(Serialize)]
pub struct DownloadItem {
    pub id: String,
    pub manga_title: String,
    pub chapter_title: String,
    pub chapter_url: String,
    pub module_id: String,
    pub format: String,
    pub total_pages: i64,
    pub downloaded_pages: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub max_retries: i64,
    pub created_at: String,
}

async fn list_downloads(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
) -> Json<Vec<DownloadItem>> {
    let rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            String,
            Option<String>,
            i64,
            i64,
            String,
        ),
    >(
        r#"
        SELECT id, manga_title, chapter_title, chapter_url, module_id, format,
               total_pages, downloaded_pages, status, error_message,
               COALESCE(retry_count, 0), COALESCE(max_retries, 3), created_at
        FROM downloads
        ORDER BY created_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(state.db.pool())
    .await
    .unwrap_or_default();

    let list = rows
        .into_iter()
        .map(
            |(
                id,
                manga_title,
                chapter_title,
                chapter_url,
                module_id,
                format,
                total_pages,
                downloaded_pages,
                status,
                error_message,
                retry_count,
                max_retries,
                created_at,
            )| {
                DownloadItem {
                    id,
                    manga_title,
                    chapter_title,
                    chapter_url,
                    module_id,
                    format,
                    total_pages,
                    downloaded_pages,
                    status,
                    error_message,
                    retry_count,
                    max_retries,
                    created_at,
                }
            },
        )
        .collect();

    Json(list)
}

async fn queue_downloads(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    Json(req): Json<QueueDownloadRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_download {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_download' requise".into(),
        ));
    }
    let format = req.format.unwrap_or_else(|| "Cbz".into());
    let mut queued = 0;

    if let Some(ref m_url) = req.manga_url {
        let manga_id = format!("{}:{}", req.module_id, m_url);
        let _ = state
            .db
            .upsert_manga_metadata(
                &manga_id,
                &req.module_id,
                &req.manga_title,
                m_url,
                req.cover_url.as_deref(),
                req.author.as_deref(),
                req.description.as_deref(),
                req.genres.as_deref(),
            )
            .await;
    }

    for ch in req.chapters {
        let dl_id = uuid::Uuid::new_v4().to_string();
        let mod_id = ch.module_id.as_deref().unwrap_or(&req.module_id);
        let res = sqlx::query(
            r#"
            INSERT INTO downloads (id, manga_title, chapter_title, chapter_url, module_id, format, status)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Pending')
            "#
        )
        .bind(&dl_id)
        .bind(&req.manga_title)
        .bind(&ch.title)
        .bind(&ch.url)
        .bind(mod_id)
        .bind(&format)
        .execute(state.db.pool())
        .await;

        if res.is_ok() {
            queued += 1;
            if let Some(ref js) = state.nats_context {
                let task = fmd_core::TaskMessage::new_download(
                    dl_id.clone(),
                    req.manga_title.clone(),
                    ch.title.clone(),
                    ch.url.clone(),
                    mod_id.to_string(),
                    format.clone(),
                    0,
                    3,
                );
                if let Ok(bytes) = serde_json::to_vec(&task) {
                    let _ = js
                        .publish(fmd_core::SUBJECT_TASK_DOWNLOAD, bytes.into())
                        .await;
                }
            }
        }
    }

    let _ = state.ws_sender.send(WsNotification {
        event: "download_queued".into(),
        payload: serde_json::json!({ "count": queued }),
    });

    Ok(Json(
        serde_json::json!({ "success": true, "queued": queued }),
    ))
}

async fn remove_download(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_download {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_download' requise".into(),
        ));
    }
    sqlx::query("DELETE FROM downloads WHERE id = ?1")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _ = state.ws_sender.send(WsNotification {
        event: "download_removed".into(),
        payload: serde_json::json!({ "id": id }),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn retry_download(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_download {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_download' requise".into(),
        ));
    }
    sqlx::query("UPDATE downloads SET status = 'Pending', error_message = NULL, downloaded_pages = 0, retry_count = 0, created_at = CURRENT_TIMESTAMP WHERE id = ?1")
        .bind(&id)
        .execute(state.db.pool())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if let Some(ref js) = state.nats_context {
        if let Ok(Some((dl_id, manga_title, chapter_title, chapter_url, module_id, format_str, retry_count, max_retries))) =
            sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64)>(
                "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE id = ?1"
            )
            .bind(&id)
            .fetch_optional(state.db.pool())
            .await
        {
            let task = fmd_core::TaskMessage::new_download(
                dl_id,
                manga_title,
                chapter_title,
                chapter_url,
                module_id,
                format_str,
                retry_count as u32,
                max_retries as u32,
            );
            if let Ok(bytes) = serde_json::to_vec(&task) {
                let _ = js.publish(fmd_core::SUBJECT_TASK_DOWNLOAD, bytes.into()).await;
            }
        }
    }

    info!("Manual download retry queued for [{}]", id);

    let _ = state.ws_sender.send(WsNotification {
        event: "download_queued".into(),
        payload: serde_json::json!({ "id": id }),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn retry_all_failed_downloads(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_download {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_download' requise".into(),
        ));
    }
    let res = sqlx::query("UPDATE downloads SET status = 'Pending', error_message = NULL, downloaded_pages = 0, retry_count = 0, created_at = CURRENT_TIMESTAMP WHERE status = 'Failed'")
        .execute(state.db.pool())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let count = res.rows_affected();
    info!("Manual retry triggered for {} failed download(s)", count);

    if let Some(ref js) = state.nats_context {
        if let Ok(failed_list) = sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64)>(
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE status = 'Pending'"
        )
        .fetch_all(state.db.pool())
        .await
        {
            for (dl_id, manga_title, chapter_title, chapter_url, module_id, format_str, retry_count, max_retries) in failed_list {
                let task = fmd_core::TaskMessage::new_download(
                    dl_id,
                    manga_title,
                    chapter_title,
                    chapter_url,
                    module_id,
                    format_str,
                    retry_count as u32,
                    max_retries as u32,
                );
                if let Ok(bytes) = serde_json::to_vec(&task) {
                    let _ = js.publish(fmd_core::SUBJECT_TASK_DOWNLOAD, bytes.into()).await;
                }
            }
        }
    }

    let _ = state.ws_sender.send(WsNotification {
        event: "download_queued".into(),
        payload: serde_json::json!({ "count": count }),
    });

    Ok(Json(serde_json::json!({ "success": true, "count": count })))
}

async fn internal_task_progress(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<fmd_core::TaskProgressUpdate>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    verify_worker_token(&headers, &state.internal_worker_token)?;

    let _ = sqlx::query(
        "UPDATE downloads SET status = ?1, downloaded_pages = ?2, total_pages = COALESCE(?3, total_pages), error_message = ?4 WHERE id = ?5"
    )
    .bind(&req.status)
    .bind(req.downloaded_pages as i64)
    .bind(req.total_pages.map(|t| t as i64))
    .bind(&req.message)
    .bind(&req.task_id)
    .execute(state.db.pool())
    .await;

    let _ = state.ws_sender.send(WsNotification {
        event: "download_progress".into(),
        payload: serde_json::json!({
            "id": &req.task_id,
            "status": &req.status,
            "downloaded_pages": req.downloaded_pages,
            "total_pages": req.total_pages,
        }),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn internal_task_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<fmd_core::TaskCompleteUpdate>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    verify_worker_token(&headers, &state.internal_worker_token)?;
    if req.success {
        let _ = sqlx::query(
            "UPDATE downloads SET status = 'Completed', downloaded_pages = total_pages, error_message = NULL WHERE id = ?1"
        )
        .bind(&req.task_id)
        .execute(state.db.pool())
        .await;

        let ch_url =
            sqlx::query_scalar::<_, String>("SELECT chapter_url FROM downloads WHERE id = ?1")
                .bind(&req.task_id)
                .fetch_optional(state.db.pool())
                .await
                .ok()
                .flatten();

        if let Some(url) = ch_url {
            let _ = state.db.mark_chapter_downloaded("", &url).await;
        }

        let _ = state.ws_sender.send(WsNotification {
            event: "download_completed".into(),
            payload: serde_json::json!({
                "id": &req.task_id,
                "status": "Completed",
                "saved_path": req.saved_path,
            }),
        });
    } else {
        let _ =
            sqlx::query("UPDATE downloads SET status = 'Failed', error_message = ?1 WHERE id = ?2")
                .bind(
                    req.error_message
                        .as_deref()
                        .unwrap_or("Worker task execution failure"),
                )
                .bind(&req.task_id)
                .execute(state.db.pool())
                .await;

        let _ = state.ws_sender.send(WsNotification {
            event: "download_failed".into(),
            payload: serde_json::json!({
                "id": &req.task_id,
                "status": "Failed",
                "error_message": req.error_message,
            }),
        });
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

#[derive(Deserialize)]
pub struct CatalogQuery {
    pub q: Option<String>,
    pub module_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct CatalogResponse {
    pub items: Vec<fmd_core::models::CatalogItem>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

async fn search_catalog(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<CatalogQuery>,
) -> Json<CatalogResponse> {
    let q = params.q.unwrap_or_default();
    let m_id = params.module_id.filter(|s| !s.is_empty() && s != "all");
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let offset = params.offset.unwrap_or(0).max(0);

    let (items, total) = state
        .cache_db()
        .search_catalog(&q, m_id.as_deref(), limit, offset)
        .await
        .unwrap_or_default();

    Json(CatalogResponse {
        items,
        total,
        limit,
        offset,
    })
}

#[derive(Serialize)]
pub struct CatalogStatusResponse {
    pub active: Option<ActiveSyncInfo>,
    pub modules: Vec<fmd_core::models::ModuleSyncStatus>,
    pub total_catalog_items: i64,
}

async fn get_catalog_status(
    _auth: auth::RequireAuth,
    State(state): State<AppState>,
) -> Json<CatalogStatusResponse> {
    let active = state.sync_manager.get_active_info().await;
    let modules = state
        .cache_db()
        .list_all_sync_statuses()
        .await
        .unwrap_or_default();
    let total_catalog_items = state
        .cache_db()
        .get_catalog_total_count()
        .await
        .unwrap_or(0);

    Json(CatalogStatusResponse {
        active,
        modules,
        total_catalog_items,
    })
}

async fn trigger_catalog_sync(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    AxumPath(module_id): AxumPath<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_trigger_catalog_sync {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_trigger_catalog_sync' requise".into(),
        ));
    }
    if state.sync_manager.is_syncing().await {
        return Err((
            axum::http::StatusCode::CONFLICT,
            "Une synchronisation est déjà en cours.".into(),
        ));
    }

    let module = state
        .modules
        .iter()
        .find(|m| m.id == module_id || m.name.eq_ignore_ascii_case(&module_id))
        .cloned()
        .ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Module {} introuvable", module_id),
            )
        })?;

    if state.shared_cache.is_some() {
        let status = state
            .cache_db()
            .get_module_sync_status(&module.id)
            .await
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if status.is_some_and(|status| {
            status.status == "Completed"
                && status.last_synced_at.is_some_and(|time| {
                    chrono::Utc::now().signed_duration_since(time) < chrono::Duration::hours(24)
                })
        }) {
            return Ok(Json(
                serde_json::json!({"status": "cached", "module_id": module.id}),
            ));
        }
    }

    let sm = state.sync_manager.clone();
    let engine = if state.shared_cache.is_some() {
        Arc::new(Mutex::new(LuaModuleEngine::new_public_only().map_err(
            |error| {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    error.to_string(),
                )
            },
        )?))
    } else {
        state.engine.clone()
    };
    let db = state.cache_db().clone();
    let ws = state.ws_sender.clone();
    let catalog_ws = state.catalog_ws_sender.clone();

    tokio::spawn(async move {
        sm.run_sync(module, engine, db, ws, catalog_ws).await;
    });

    Ok(Json(serde_json::json!({
        "status": "started",
        "module_id": module_id
    })))
}

async fn cancel_catalog_sync(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if !ctx.permissions.can_trigger_catalog_sync {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Permission 'can_trigger_catalog_sync' requise".into(),
        ));
    }
    let cancelled = state.sync_manager.cancel().await;
    Ok(Json(serde_json::json!({ "cancelled": cancelled })))
}

#[derive(Deserialize)]
struct ProxyImageQuery {
    url: String,
    referer: Option<String>,
}

async fn proxy_image(
    _auth: auth::RequireAuth,
    Query(params): Query<ProxyImageQuery>,
) -> Result<impl IntoResponse, (axum::http::StatusCode, String)> {
    let safe_url = fmd_net::validate_safe_url(&params.url).await.map_err(|e| {
        (
            axum::http::StatusCode::FORBIDDEN,
            format!("SSRF protection rejected request: {}", e),
        )
    })?;

    let client = fmd_net::HttpClient::new_public_only()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut headers = std::collections::HashMap::new();
    if let Some(ref ref_url) = params.referer {
        if let Ok(safe_ref) = fmd_net::validate_safe_url(ref_url).await {
            headers.insert("Referer".to_string(), safe_ref.to_string());
        }
    } else if let Some(host) = safe_url.host_str() {
        headers.insert(
            "Referer".to_string(),
            format!("{}://{}/", safe_url.scheme(), host),
        );
    }

    let bytes = client
        .get_bytes_with_headers(safe_url.as_str(), &headers)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_GATEWAY,
                format!("Failed to proxy image: {}", e),
            )
        })?;

    let lower = safe_url.path().to_lowercase();
    let content_type = if lower.ends_with(".webp") || params.url.to_lowercase().contains(".webp") {
        "image/webp"
    } else if lower.ends_with(".png") || params.url.to_lowercase().contains(".png") {
        "image/png"
    } else if lower.ends_with(".gif") || params.url.to_lowercase().contains(".gif") {
        "image/gif"
    } else {
        "image/jpeg"
    };

    let mut response_headers = axum::http::HeaderMap::new();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static(content_type),
    );
    response_headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("public, max-age=86400"),
    );

    Ok((response_headers, bytes))
}

async fn get_settings_handler(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
) -> Result<Json<fmd_core::AppSettings>, (axum::http::StatusCode, String)> {
    if state.shared_cache.is_none() && !ctx.user.is_admin() {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Privilèges administrateur requis".into(),
        ));
    }
    let s = state
        .db
        .get_settings()
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(s))
}

async fn update_settings_handler(
    auth::RequireAuth(ctx): auth::RequireAuth,
    State(state): State<AppState>,
    Json(new_settings): Json<fmd_core::AppSettings>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    if state.shared_cache.is_none() && !ctx.user.is_admin() {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            "Privilèges administrateur requis".into(),
        ));
    }
    state
        .db
        .update_settings(&new_settings)
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let _ = state.ws_sender.send(WsNotification {
        event: "settings_updated".into(),
        payload: serde_json::to_value(&new_settings).unwrap_or_default(),
    });

    Ok(Json(serde_json::json!({ "success": true })))
}

#[cfg(test)]
mod tests {
    use super::with_base_path;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn accepts_prefixed_and_prefix_stripped_routes() {
        let app = Router::new()
            .route("/", get(|| async { "index" }))
            .route("/api/health", get(|| async { "OK" }));
        let app = with_base_path(app, "/reader");

        for uri in ["/reader/", "/reader/api/health", "/api/health", "/"] {
            let response = app
                .clone()
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "request to {uri}");
        }
    }

    #[tokio::test]
    async fn overlapping_base_paths_dispatch_prefixed_and_unprefixed_requests() {
        for prefix in ["/api/health", "/logo.png"] {
            let app = Router::new()
                .route("/", get(|| async { "index" }))
                .route("/api/health", get(|| async { "health" }))
                .route("/logo.png", get(|| async { "logo" }))
                .route(
                    "/item/{id}",
                    axum::routing::post(|AxumPath(id): AxumPath<String>| async move { id }),
                )
                .route(
                    "/query",
                    get(|uri: axum::http::Uri| async move {
                        uri.query().unwrap_or_default().to_owned()
                    }),
                )
                .fallback(get(|| async { "fallback" }));
            let app = with_base_path(app, prefix);
            for (uri, expected) in [
                (prefix.to_string(), "index"),
                (format!("{prefix}/"), "index"),
                (format!("{prefix}/api/health?check=1"), "health"),
                (format!("{prefix}/logo.png"), "logo"),
                (format!("{prefix}/query?check=1"), "check=1"),
                ("/query?check=2".to_string(), "check=2"),
                (format!("{prefix}suffix"), "fallback"),
                ("/".to_string(), "index"),
            ] {
                let response = app
                    .clone()
                    .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK, "request to {uri}");
                let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                assert_eq!(&body[..], expected.as_bytes(), "request to {uri}");
            }
            let uri = format!("{prefix}/item/123");
            let response = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "request to {uri}");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(&body[..], b"123");
        }
    }

    #[tokio::test]
    async fn empty_base_path_keeps_root_routes() {
        let app = Router::new().route("/api/health", get(|| async { "OK" }));
        let response = with_base_path(app, "")
            .oneshot(
                Request::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    use super::*;

    #[tokio::test]
    async fn test_download_failure_retry_and_permanent_failure() {
        let db = Database::new(":memory:").await.unwrap();
        let engine = Arc::new(Mutex::new(LuaModuleEngine::new().unwrap()));
        let (ws_sender, _) = broadcast::channel(10);
        let sync_manager = Arc::new(CatalogSyncManager::new());
        let state = AppState {
            db: db.clone(),
            shared_cache: None,
            demo_shutdown: None,
            engine,
            modules: Arc::new(vec![]),
            ws_sender,
            catalog_ws_sender: None,
            sync_manager,
            nats_context: None,
            throttler: auth::DomainThrottler::new(),
            login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
            internal_worker_token: "test_internal_token".to_string(),
        };

        let dl_id = "test-dl-1";
        sqlx::query(
            "INSERT INTO downloads (id, manga_title, chapter_title, chapter_url, module_id, format, status, retry_count, max_retries) VALUES (?1, 'Manga A', 'Ch. 1', 'https://example.com/1', 'test_mod', 'Cbz', 'Downloading', 0, 3)"
        )
        .bind(dl_id)
        .execute(db.pool())
        .await
        .unwrap();

        // 1st failure: attempt 1 of 3 -> should become Pending with retry_count = 1
        handle_download_failure(&state, dl_id, "Manga A", "Ch. 1", 0, 3, "HTTP 500").await;

        let row1 = sqlx::query_as::<_, (String, i64, Option<String>)>(
            "SELECT status, retry_count, error_message FROM downloads WHERE id = ?1",
        )
        .bind(dl_id)
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert_eq!(row1.0, "Pending");
        assert_eq!(row1.1, 1);
        assert!(row1.2.unwrap().contains("Tentative 1/3"));

        // 3rd failure (retry_count = 2, max_retries = 3) -> should become Pending with retry_count = 3
        handle_download_failure(&state, dl_id, "Manga A", "Ch. 1", 2, 3, "HTTP 503").await;

        let row2 = sqlx::query_as::<_, (String, i64, Option<String>)>(
            "SELECT status, retry_count, error_message FROM downloads WHERE id = ?1",
        )
        .bind(dl_id)
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert_eq!(row2.0, "Pending");
        assert_eq!(row2.1, 3);

        // 4th failure (retry_count = 3 >= max_retries 3) -> should become Failed
        handle_download_failure(&state, dl_id, "Manga A", "Ch. 1", 3, 3, "HTTP 504").await;

        let row3 = sqlx::query_as::<_, (String, i64, Option<String>)>(
            "SELECT status, retry_count, error_message FROM downloads WHERE id = ?1",
        )
        .bind(dl_id)
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert_eq!(row3.0, "Failed");
        assert_eq!(row3.1, 3);
        assert_eq!(row3.2.unwrap(), "HTTP 504");

        // Now test retry_all_failed_downloads
        let test_user = fmd_core::User {
            id: "test-user-1".to_string(),
            username: "admin".to_string(),
            password_hash: None,
            role: fmd_core::Role::Admin,
            is_active: true,
            sso_provider: None,
            sso_subject: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_login_at: None,
        };
        let test_ctx = fmd_core::AuthContext {
            user: test_user,
            permissions: fmd_core::UserPermissions::admin_defaults("test-user-1"),
            is_api_token: false,
        };

        let res = retry_all_failed_downloads(auth::RequireAuth(test_ctx), State(state.clone()))
            .await
            .unwrap();
        assert_eq!(res.0["count"], 1);

        let row4 = sqlx::query_as::<_, (String, i64, Option<String>)>(
            "SELECT status, retry_count, error_message FROM downloads WHERE id = ?1",
        )
        .bind(dl_id)
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert_eq!(row4.0, "Pending");
        assert_eq!(row4.1, 0);
        assert_eq!(row4.2, None);
    }

    #[tokio::test]
    async fn test_manga_metadata_upsert_and_comic_info_injection() {
        let db = Database::new(":memory:").await.unwrap();

        let manga_id = "test_mod:https://example.com/one-piece";
        let genres = vec![
            "Action".to_string(),
            "Aventure".to_string(),
            "Comédie".to_string(),
        ];

        db.upsert_manga_metadata(
            manga_id,
            "test_mod",
            "One Piece",
            "https://example.com/one-piece",
            Some("https://example.com/cover.jpg"),
            Some("Eiichiro Oda"),
            Some("L'histoire de Monkey D. Luffy"),
            Some(&genres),
        )
        .await
        .unwrap();

        // Query metadata from database
        let prefix_id = "test_mod:%";
        let manga_meta = sqlx::query_as::<
            _,
            (
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(
            r#"
            SELECT author, genres, description, original_url
            FROM mangas
            WHERE title = ?1 OR id LIKE ?2
            ORDER BY (author IS NOT NULL AND author != '') DESC, updated_at DESC
            LIMIT 1
            "#,
        )
        .bind("One Piece")
        .bind(prefix_id)
        .fetch_optional(db.pool())
        .await
        .unwrap()
        .unwrap();

        assert_eq!(manga_meta.0.as_deref(), Some("Eiichiro Oda"));
        assert_eq!(
            manga_meta.2.as_deref(),
            Some("L'histoire de Monkey D. Luffy")
        );
        assert_eq!(
            manga_meta.3.as_deref(),
            Some("https://example.com/one-piece")
        );

        let genres_vec = manga_meta
            .1
            .as_ref()
            .map(|g| serde_json::from_str::<Vec<String>>(g).unwrap_or_default())
            .unwrap_or_default();

        assert_eq!(genres_vec, vec!["Action", "Aventure", "Comédie"]);

        // Pack chapter and verify ComicInfo.xml
        let mut packer = fmd_core::ChapterPacker::new("One Piece".into(), "Chapitre 1180".into());
        packer.set_metadata(fmd_core::ChapterMetadata {
            series: "One Piece".into(),
            title: "Chapitre 1180".into(),
            number: Some("1180".into()),
            summary: manga_meta.2.clone(),
            writer: manga_meta.0.clone(),
            genres: genres_vec,
            web: manga_meta.3.clone(),
            language_iso: Some("fr".to_string()),
        });

        let xml = packer.generate_comic_info_xml(15);
        assert!(xml.contains("<Series>One Piece</Series>"));
        assert!(xml.contains("<Number>1180</Number>"));
        assert!(xml.contains("<Writer>Eiichiro Oda</Writer>"));
        assert!(xml.contains("<Summary>L&apos;histoire de Monkey D. Luffy</Summary>"));
        assert!(xml.contains("<Genre>Action, Aventure, Comédie</Genre>"));
        assert!(xml.contains("<Tags>Action, Aventure, Comédie</Tags>"));
        assert!(xml.contains("<Manga>YesAndRightToLeft</Manga>"));
        assert!(xml.contains("<PageCount>15</PageCount>"));
    }

    #[tokio::test]
    async fn test_queue_multi_source_downloads() {
        let db = Database::new(":memory:").await.unwrap();
        let engine = Arc::new(Mutex::new(LuaModuleEngine::new().unwrap()));
        let (ws_sender, _) = broadcast::channel(10);
        let sync_manager = Arc::new(CatalogSyncManager::new());
        let state = AppState {
            db: db.clone(),
            shared_cache: None,
            demo_shutdown: None,
            engine,
            modules: Arc::new(vec![]),
            ws_sender,
            catalog_ws_sender: None,
            sync_manager,
            nats_context: None,
            throttler: auth::DomainThrottler::new(),
            login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
            internal_worker_token: "test_internal_token".to_string(),
        };

        let req = QueueDownloadRequest {
            module_id: "mangaplus".to_string(),
            manga_title: "Jujutsu Kaisen".to_string(),
            manga_url: Some("https://mangaplus.shueisha.co.jp/titles/100034".to_string()),
            author: Some("Gege Akutami".to_string()),
            genres: Some(vec!["Action".to_string(), "Surnaturel".to_string()]),
            description: Some("Yuji Itadori...".to_string()),
            cover_url: None,
            format: Some("Cbz".to_string()),
            chapters: vec![
                QueueChapterItem {
                    title: "Chapitre 1".to_string(),
                    url: "mp/1".to_string(),
                    module_id: Some("mangaplus".to_string()),
                },
                QueueChapterItem {
                    title: "Chapitre 4".to_string(),
                    url: "ss/4".to_string(),
                    module_id: Some("sushiscan".to_string()),
                },
            ],
        };

        let test_user = fmd_core::User {
            id: "test-user-1".to_string(),
            username: "admin".to_string(),
            password_hash: None,
            role: fmd_core::Role::Admin,
            is_active: true,
            sso_provider: None,
            sso_subject: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_login_at: None,
        };
        let test_ctx = fmd_core::AuthContext {
            user: test_user,
            permissions: fmd_core::UserPermissions::admin_defaults("test-user-1"),
            is_api_token: false,
        };

        let res = queue_downloads(auth::RequireAuth(test_ctx), State(state.clone()), Json(req))
            .await
            .unwrap();
        assert_eq!(res.0["queued"], 2);

        let items = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT chapter_title, chapter_url, module_id, format FROM downloads ORDER BY chapter_title ASC"
        )
        .fetch_all(db.pool())
        .await
        .unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "Chapitre 1");
        assert_eq!(items[0].2, "mangaplus");
        assert_eq!(items[1].0, "Chapitre 4");
        assert_eq!(items[1].2, "sushiscan");
    }

    #[tokio::test]
    async fn test_bootstrap_from_env_and_auth_status() {
        let db = Database::new(":memory:").await.unwrap();
        let engine = Arc::new(Mutex::new(LuaModuleEngine::new().unwrap()));
        let (ws_sender, _) = broadcast::channel(10);
        let sync_manager = Arc::new(CatalogSyncManager::new());
        let state = AppState {
            db: db.clone(),
            shared_cache: None,
            demo_shutdown: None,
            engine,
            modules: Arc::new(vec![]),
            ws_sender,
            catalog_ws_sender: None,
            sync_manager,
            nats_context: None,
            throttler: auth::DomainThrottler::new(),
            login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
            internal_worker_token: "test_internal_token".to_string(),
        };

        // Initially no users -> setup_needed must be true
        let status1 = auth::get_auth_status(State(state.clone()), auth::MaybeAuth(None)).await;
        assert!(status1.setup_needed);
        assert!(!status1.authenticated);

        // Simulate environment variables injected by Kubernetes Secret
        std::env::set_var("FMD_ADMIN_USER", "k8s_admin");
        std::env::set_var("FMD_ADMIN_PASSWORD", "SuperSecurePassword123!");

        auth::bootstrap_admin_from_env_or_secret(&state.db)
            .await
            .unwrap();
        assert_eq!(state.db.count_users().await.unwrap(), 1);

        // After bootstrap -> setup_needed must be false (wizard bypassed)
        let status2 = auth::get_auth_status(State(state.clone()), auth::MaybeAuth(None)).await;
        assert!(!status2.setup_needed);

        // Login with bootstrapped admin credentials
        let (headers, login_res) = auth::auth_login(
            State(state.clone()),
            axum::http::HeaderMap::new(),
            Json(auth::LoginRequest {
                username: "k8s_admin".into(),
                password: "SuperSecurePassword123!".into(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(login_res["status"], "ok");
        assert_eq!(login_res["user"]["username"], "k8s_admin");
        assert_eq!(login_res["user"]["role"], "admin");
        assert!(headers.get("set-cookie").is_some());

        // Test logout
        let mut req_headers = axum::http::HeaderMap::new();
        let cookie_str = headers.get("set-cookie").unwrap().to_str().unwrap();
        req_headers.insert(
            "cookie",
            axum::http::HeaderValue::from_str(cookie_str).unwrap(),
        );

        let (logout_headers, logout_res) =
            auth::auth_logout(State(state.clone()), req_headers).await;
        assert_eq!(logout_res["status"], "ok");
        let logout_cookie = logout_headers.get("set-cookie").unwrap().to_str().unwrap();
        assert!(logout_cookie.contains("fmd_session=;"));
        assert!(logout_cookie.contains("Max-Age=0"));

        // Clean up env vars
        std::env::remove_var("FMD_ADMIN_USER");
        std::env::remove_var("FMD_ADMIN_PASSWORD");
    }

    #[tokio::test]
    async fn test_login_rate_limiter_brute_force_lockout() {
        let limiter = auth::LoginRateLimiter::new();
        let test_ip = "192.0.2.1"; // TEST-NET-1

        // 4 failed attempts -> should still be allowed
        for _ in 0..4 {
            assert!(limiter.check_allowed(test_ip).await.is_ok());
            limiter.record_failure(test_ip).await;
        }
        assert!(limiter.check_allowed(test_ip).await.is_ok());

        // 5th failure -> triggers lockout
        limiter.record_failure(test_ip).await;
        let res = limiter.check_allowed(test_ip).await;
        assert!(res.is_err());
        let remaining = res.unwrap_err();
        assert!(remaining > 0 && remaining <= 60);

        // Success clears lock
        limiter.record_success(test_ip).await;
        assert!(limiter.check_allowed(test_ip).await.is_ok());
    }

    #[tokio::test]
    async fn test_internal_worker_token_verification() {
        let mut valid_headers = axum::http::HeaderMap::new();
        valid_headers.insert(
            "x-fmd-internal-token",
            axum::http::HeaderValue::from_static("secret_token_123"),
        );

        let mut invalid_headers = axum::http::HeaderMap::new();
        invalid_headers.insert(
            "x-fmd-internal-token",
            axum::http::HeaderValue::from_static("wrong_token"),
        );

        let empty_headers = axum::http::HeaderMap::new();

        assert!(verify_worker_token(&valid_headers, "secret_token_123").is_ok());
        assert!(verify_worker_token(&invalid_headers, "secret_token_123").is_err());
        assert!(verify_worker_token(&empty_headers, "secret_token_123").is_err());
        assert!(verify_worker_token(&empty_headers, "").is_err());
        assert!(verify_worker_token(&valid_headers, "").is_err());
    }

    #[tokio::test]
    async fn test_ssrf_proxy_image_blocked() {
        let test_user = fmd_core::User {
            id: "u1".into(),
            username: "tester".into(),
            password_hash: None,
            role: fmd_core::Role::Admin,
            is_active: true,
            sso_provider: None,
            sso_subject: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_login_at: None,
        };
        let auth_ctx = auth::RequireAuth(fmd_core::AuthContext {
            user: test_user,
            permissions: fmd_core::UserPermissions::admin_defaults("u1"),
            is_api_token: false,
        });

        // Loopback SSRF probe
        let res1 = proxy_image(
            auth_ctx,
            axum::extract::Query(ProxyImageQuery {
                url: "http://127.0.0.1:8088/api/users".into(),
                referer: None,
            }),
        )
        .await;
        assert!(res1.is_err());
        assert_eq!(res1.err().unwrap().0, axum::http::StatusCode::FORBIDDEN);
    }
}
