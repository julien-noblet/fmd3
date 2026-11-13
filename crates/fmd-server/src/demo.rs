use anyhow::{bail, Context};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Json, Router,
};
use fmd_core::{auth::generate_secure_token, Database, Role, UserPermissions};
use fmd_engine_lua::LuaModuleEngine;
use serde_json::json;
use std::{collections::VecDeque, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch, Mutex};
use tokio::time::Instant;
use tower::ServiceExt;
use tracing::{info, warn};

use crate::{auth, build_router, AppState, CatalogSyncManager};

const IDLE_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const QUEUE_TIMEOUT: Duration = Duration::from_secs(90);

struct Visitor {
    key: String,
    last_seen: Instant,
    slot: Option<Slot>,
}

struct Slot {
    username: String,
    password: String,
    logged_in: bool,
    db: Database,
    app: Router,
    shutdown: watch::Sender<bool>,
}

struct Manager {
    slots: usize,
    visitors: VecDeque<Visitor>,
    base: AppState,
    base_path: String,
}

impl Manager {
    async fn expire(&mut self) {
        let now = Instant::now();
        let mut index = 0;
        while index < self.visitors.len() {
            let visitor = &self.visitors[index];
            let timeout = if visitor.slot.is_some() {
                IDLE_TIMEOUT
            } else {
                QUEUE_TIMEOUT
            };
            if now.duration_since(visitor.last_seen) >= timeout {
                if let Some(visitor) = self.visitors.remove(index) {
                    close_visitor(visitor).await;
                }
            } else {
                index += 1;
            }
        }
    }

    async fn assign(&mut self) -> anyhow::Result<()> {
        let used = self.visitors.iter().filter(|v| v.slot.is_some()).count();
        for _ in used..self.slots {
            let Some(index) = self.visitors.iter().position(|v| v.slot.is_none()) else {
                break;
            };
            let password = generate_secure_token();
            let username = format!("demo-{}", uuid::Uuid::new_v4().simple());
            let db = Database::new_in_memory().await?;
            let permissions = UserPermissions {
                can_download: false,
                can_manage_favorites: true,
                can_trigger_catalog_sync: true,
                can_force_refresh: false,
                can_view_modules: true,
                ..UserPermissions::default()
            };
            db.create_user(
                &username,
                Some(&password),
                Role::User,
                Some(permissions),
                None,
                None,
            )
            .await?;
            let mut state = self.base.clone();
            state.db = db.clone();
            let (sender, _) = broadcast::channel(100);
            state.ws_sender = sender;
            let (shutdown, receiver) = watch::channel(false);
            state.demo_shutdown = Some(receiver);
            let app = build_router(state, &self.base_path);
            let visitor = &mut self.visitors[index];
            visitor.last_seen = Instant::now();
            visitor.slot = Some(Slot {
                username,
                password,
                logged_in: false,
                db,
                app,
                shutdown,
            });
        }
        Ok(())
    }
}

async fn close_visitor(visitor: Visitor) {
    if let Some(slot) = visitor.slot {
        let _ = slot.shutdown.send(true);
        slot.db.pool().close().await;
    }
}

type SharedManager = Arc<Mutex<Manager>>;

fn browser_key(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| part.trim().strip_prefix("fmd_demo="))
}

fn json_response(status: StatusCode, value: serde_json::Value) -> Response {
    (status, Json(value)).into_response()
}

async fn dispatch(State(manager): State<SharedManager>, req: Request) -> Response {
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let key = browser_key(req.headers()).map(str::to_owned);

    if path == "/api/demo/info" && method == Method::GET {
        return json_response(StatusCode::OK, json!({"demo": true}));
    }
    if path == "/api/demo/reservation" && method == Method::POST {
        let mut manager = manager.lock().await;
        manager.expire().await;
        let key = key
            .filter(|key| manager.visitors.iter().any(|v| &v.key == key))
            .unwrap_or_else(generate_secure_token);
        if let Some(visitor) = manager.visitors.iter_mut().find(|v| v.key == key) {
            if visitor.slot.is_none() {
                visitor.last_seen = Instant::now();
            }
        } else {
            manager.visitors.push_back(Visitor {
                key: key.clone(),
                last_seen: Instant::now(),
                slot: None,
            });
        }
        if let Err(error) = manager.assign().await {
            tracing::error!("Failed to create demo slot: {error:#}");
            return json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"error": "Demo slot unavailable"}),
            );
        }
        let visitor = manager
            .visitors
            .iter()
            .find(|v| v.key == key)
            .expect("visitor exists");
        let body = if let Some(slot) = &visitor.slot {
            if slot.logged_in {
                json!({"status": "active"})
            } else {
                json!({"status": "reserved", "username": slot.username, "password": slot.password})
            }
        } else {
            let position = manager
                .visitors
                .iter()
                .filter(|v| v.slot.is_none())
                .position(|v| v.key == key)
                .expect("queued visitor exists")
                + 1;
            json!({"status": "waiting", "position": position})
        };
        let mut response = json_response(StatusCode::OK, body);
        let secure = if auth::is_request_secure(req.headers()) {
            "; Secure"
        } else {
            ""
        };
        let cookie = format!("fmd_demo={key}; HttpOnly; Path=/; SameSite=Lax{secure}");
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&cookie).expect("valid cookie"),
        );
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return response;
    }
    if path == "/api/demo/activity" && method == Method::POST {
        let mut manager = manager.lock().await;
        manager.expire().await;
        let Some(visitor) = manager
            .visitors
            .iter_mut()
            .find(|v| Some(v.key.as_str()) == key.as_deref())
        else {
            return json_response(StatusCode::UNAUTHORIZED, json!({"error": "Demo expired"}));
        };
        if let Some(slot) = &visitor.slot {
            if slot.logged_in {
                let session = req
                    .headers()
                    .get(header::COOKIE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| {
                        v.split(';')
                            .find_map(|part| part.trim().strip_prefix("fmd_session="))
                    });
                if let Some(token) = session {
                    if !matches!(slot.db.validate_session_or_token(token).await, Ok(Some(_))) {
                        return json_response(
                            StatusCode::UNAUTHORIZED,
                            json!({"error": "Demo session expired"}),
                        );
                    }
                } else {
                    return json_response(
                        StatusCode::UNAUTHORIZED,
                        json!({"error": "Demo session required"}),
                    );
                }
            }
            visitor.last_seen = Instant::now();
            return json_response(StatusCode::OK, json!({"status": "ok"}));
        }
        return json_response(
            StatusCode::UNAUTHORIZED,
            json!({"error": "Demo reservation required"}),
        );
    }

    if path.starts_with("/api/admin/")
        || path.starts_with("/api/auth/tokens")
        || path.starts_with("/api/internal/")
        || path == "/api/catalog/sync/cancel"
        || path == "/api/auth/setup"
        || (path.starts_with("/api/downloads") && method != Method::GET)
    {
        return json_response(
            StatusCode::FORBIDDEN,
            json!({"error": "Unavailable in demo mode"}),
        );
    }

    let mut manager = manager.lock().await;
    manager.expire().await;
    let visitor = manager
        .visitors
        .iter_mut()
        .find(|v| Some(v.key.as_str()) == key.as_deref());
    if path == "/api/auth/login" {
        let Some(visitor) = visitor else {
            return json_response(
                StatusCode::UNAUTHORIZED,
                json!({"error": "Demo reservation required"}),
            );
        };
        let Some(slot) = visitor.slot.as_mut() else {
            return json_response(
                StatusCode::UNAUTHORIZED,
                json!({"error": "Demo reservation required"}),
            );
        };
        if slot.logged_in {
            return json_response(
                StatusCode::CONFLICT,
                json!({"error": "Demo account already in use"}),
            );
        }
        let mut response = match slot.app.clone().oneshot(req).await {
            Ok(response) => response,
            Err(error) => match error {},
        };
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if response.status().is_success() {
            slot.logged_in = true;
            slot.password.clear();
            visitor.last_seen = Instant::now();
        }
        return response;
    }
    if path == "/api/auth/logout" && method == Method::POST {
        if let Some(index) = manager
            .visitors
            .iter()
            .position(|v| Some(v.key.as_str()) == key.as_deref())
        {
            if let Some(visitor) = manager.visitors.remove(index) {
                close_visitor(visitor).await;
            }
        }
        let mut response = json_response(StatusCode::OK, json!({"status": "ok"}));
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_static("fmd_session=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0"),
        );
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_static("fmd_demo=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0"),
        );
        return response;
    }
    let app = visitor
        .and_then(|v| v.slot.as_ref())
        .map(|slot| slot.app.clone());
    let fallback = if app.is_none() {
        Some(build_router(manager.base.clone(), &manager.base_path))
    } else {
        None
    };
    drop(manager);
    if path.starts_with("/api/") && app.is_none() {
        if path == "/api/auth/status" {
            return json_response(
                StatusCode::OK,
                json!({"setup_needed": false, "authenticated": false, "user": null, "permissions": null}),
            );
        }
        if path != "/api/health" && path != "/api/version" {
            return json_response(
                StatusCode::UNAUTHORIZED,
                json!({"error": "Demo reservation required"}),
            );
        }
    }
    match app
        .or(fallback)
        .expect("slot or fallback router")
        .oneshot(req)
        .await
    {
        Ok(response) => response,
        Err(error) => match error {},
    }
}

pub async fn run_server(
    addr: SocketAddr,
    config: DemoConfig,
    base_path: &str,
) -> anyhow::Result<()> {
    fn reject_symlink(path: &Path) -> anyhow::Result<()> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "Demo storage path must not be a symlink: {}",
                    path.display()
                )
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).with_context(|| format!("Cannot inspect {}", path.display())),
        }
    }

    let dir = Path::new("data/fmd-demo");
    reject_symlink(dir)?;
    std::fs::create_dir_all(dir).context("Failed to create demo data directory")?;
    let cache_path = dir.join("public-cache.db");
    reject_symlink(&cache_path)?;
    let cache = Database::new(cache_path).await?;
    let engine_raw = LuaModuleEngine::new().map_err(|e| anyhow::anyhow!("{e}"))?;
    let modules = if Path::new("lua/modules").exists() {
        engine_raw
            .scan_modules_dir("lua/modules")
            .map_err(|e| anyhow::anyhow!("{e}"))?
    } else {
        Vec::new()
    };
    let (ws_sender, _) = broadcast::channel(100);
    let base = AppState {
        db: Database::new_in_memory().await?,
        shared_cache: Some(cache),
        demo_shutdown: None,
        engine: Arc::new(Mutex::new(engine_raw)),
        modules: Arc::new(modules),
        ws_sender,
        sync_manager: Arc::new(CatalogSyncManager::new()),
        nats_context: None,
        throttler: auth::DomainThrottler::new(),
        login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
        internal_worker_token: generate_secure_token(),
    };
    let manager = Arc::new(Mutex::new(Manager {
        slots: config.slots,
        visitors: VecDeque::new(),
        base,
        base_path: base_path.to_owned(),
    }));
    let cleanup = manager.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let mut manager = cleanup.lock().await;
            manager.expire().await;
            if let Err(error) = manager.assign().await {
                warn!("Failed to assign demo slot: {error:#}");
            }
        }
    });
    let app = crate::with_base_path(
        Router::new()
            .fallback(any(dispatch))
            .layer(axum::middleware::from_fn(crate::add_security_headers))
            .with_state(manager),
        base_path,
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Starting demo server on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct DemoConfig {
    pub slots: usize,
}

impl DemoConfig {
    fn parse(mode: Option<&str>, slots: Option<&str>) -> anyhow::Result<Option<Self>> {
        let enabled = match mode {
            None | Some("false") => false,
            Some("true") => true,
            Some(value) => bail!("Invalid FMD_DEMO_MODE value {value:?}: expected true or false"),
        };
        if !enabled {
            return Ok(None);
        }

        let slots = match slots {
            Some(value) => value
                .parse::<usize>()
                .context("DEMO_SLOTS must be a positive integer")?,
            None => 10,
        };
        if slots == 0 {
            bail!("DEMO_SLOTS must be a positive integer");
        }
        Ok(Some(Self { slots }))
    }

    pub fn from_env() -> anyhow::Result<Option<Self>> {
        fn read(name: &str) -> anyhow::Result<Option<String>> {
            match std::env::var(name) {
                Ok(value) => Ok(Some(value)),
                Err(std::env::VarError::NotPresent) => Ok(None),
                Err(error) => Err(error).with_context(|| format!("Invalid {name}")),
            }
        }

        let mode = read("FMD_DEMO_MODE")?;
        let slots = if mode.as_deref() == Some("true") {
            read("DEMO_SLOTS")?
        } else {
            None
        };
        Self::parse(mode.as_deref(), slots.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::Request,
    };

    async fn manager(slots: usize) -> SharedManager {
        let (ws_sender, _) = broadcast::channel(10);
        Arc::new(Mutex::new(Manager {
            slots,
            visitors: VecDeque::new(),
            base_path: String::new(),
            base: AppState {
                db: Database::new_in_memory().await.unwrap(),
                shared_cache: Some(Database::new_in_memory().await.unwrap()),
                demo_shutdown: None,
                engine: Arc::new(Mutex::new(LuaModuleEngine::new().unwrap())),
                modules: Arc::new(vec![]),
                ws_sender,
                sync_manager: Arc::new(CatalogSyncManager::new()),
                nats_context: None,
                throttler: auth::DomainThrottler::new(),
                login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
                internal_worker_token: generate_secure_token(),
            },
        }))
    }

    async fn call(
        manager: &SharedManager,
        method: Method,
        path: &str,
        cookie: Option<&str>,
        body: &str,
    ) -> Response {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, cookie);
        }
        let app = Router::new()
            .fallback(any(dispatch))
            .with_state(manager.clone());
        app.oneshot(builder.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap()
    }

    async fn reserve(manager: &SharedManager, cookie: Option<&str>) -> (String, serde_json::Value) {
        let response = call(manager, Method::POST, "/api/demo/reservation", cookie, "").await;
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (cookie, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn parses_demo_configuration() {
        assert_eq!(DemoConfig::parse(None, None).unwrap(), None);
        assert_eq!(DemoConfig::parse(Some("false"), Some("bad")).unwrap(), None);
        assert_eq!(
            DemoConfig::parse(Some("true"), None).unwrap(),
            Some(DemoConfig { slots: 10 })
        );
        assert_eq!(
            DemoConfig::parse(Some("true"), Some("3")).unwrap(),
            Some(DemoConfig { slots: 3 })
        );
    }

    #[test]
    fn rejects_invalid_demo_configuration() {
        assert!(DemoConfig::parse(Some("yes"), None).is_err());
        for value in ["0", "-1", "abc", ""] {
            assert!(DemoConfig::parse(Some("true"), Some(value)).is_err());
        }
    }

    #[tokio::test]
    async fn isolates_slots_and_keeps_public_cache_shared() {
        let manager = manager(2).await;
        let (cookie_a, a) = reserve(&manager, None).await;
        let (cookie_b, b) = reserve(&manager, None).await;
        assert_ne!(cookie_a, cookie_b);
        assert_ne!(a["username"], b["username"]);
        let manager = manager.lock().await;
        let first = manager.visitors[0].slot.as_ref().unwrap();
        let second = manager.visitors[1].slot.as_ref().unwrap();
        first
            .db
            .set_setting("test-private", "secret")
            .await
            .unwrap();
        assert_eq!(second.db.get_setting("test-private").await.unwrap(), None);
        manager
            .base
            .cache_db()
            .upsert_catalog_items("module", &[("Title".into(), "url".into())])
            .await
            .unwrap();
        assert_eq!(
            manager
                .base
                .cache_db()
                .get_catalog_total_count()
                .await
                .unwrap(),
            1
        );
        assert_eq!(second.db.get_catalog_total_count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn queue_login_logout_and_expiry_are_isolated() {
        let manager = manager(1).await;
        let (first_cookie, first) = reserve(&manager, None).await;
        let (second_cookie, second) = reserve(&manager, None).await;
        let (third_cookie, third) = reserve(&manager, None).await;
        assert_eq!(second["position"], 1);
        assert_eq!(third["position"], 2);

        let login =
            json!({"username": first["username"], "password": first["password"]}).to_string();
        let response = call(
            &manager,
            Method::POST,
            "/api/auth/login",
            Some(&first_cookie),
            &login,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let session = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let cookies = format!("{first_cookie}; {session}");
        let old_db = {
            let state = manager.lock().await;
            state.visitors[0].slot.as_ref().unwrap().db.clone()
        };
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/demo/activity",
                Some(&cookies),
                ""
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/auth/login",
                Some(&cookies),
                &login
            )
            .await
            .status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/downloads/queue",
                Some(&cookies),
                "{}"
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/auth/tokens",
                Some(&cookies),
                "{}"
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/auth/logout",
                Some(&cookies),
                ""
            )
            .await
            .status(),
            StatusCode::OK
        );
        assert!(old_db.pool().is_closed());
        let (_, next) = reserve(&manager, Some(&second_cookie)).await;
        assert_eq!(next["status"], "reserved");
        assert_ne!(next["password"], first["password"]);
        assert_eq!(
            reserve(&manager, Some(&third_cookie)).await.1["position"],
            1
        );
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/auth/login",
                Some(&cookies),
                &login
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );

        {
            let mut state = manager.lock().await;
            let visitor = state
                .visitors
                .iter_mut()
                .find(|v| second_cookie.ends_with(&v.key))
                .unwrap();
            visitor.last_seen = Instant::now() - IDLE_TIMEOUT;
        }
        let (_, next) = reserve(&manager, Some(&third_cookie)).await;
        assert_eq!(next["status"], "reserved");
    }

    #[tokio::test]
    async fn concurrent_visitors_cannot_read_each_others_favorites() {
        let manager = manager(2).await;
        let (cookie_a, a) = reserve(&manager, None).await;
        let (cookie_b, b) = reserve(&manager, None).await;
        async fn login(manager: &SharedManager, cookie: &str, creds: &serde_json::Value) -> String {
            let body =
                json!({"username": creds["username"], "password": creds["password"]}).to_string();
            let response = call(
                manager,
                Method::POST,
                "/api/auth/login",
                Some(cookie),
                &body,
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            let session = response
                .headers()
                .get(header::SET_COOKIE)
                .unwrap()
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap();
            format!("{cookie}; {session}")
        }
        let a = login(&manager, &cookie_a, &a).await;
        let b = login(&manager, &cookie_b, &b).await;
        let favorite = json!({"module_id":"test", "url":"url", "title":"private"}).to_string();
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/favorites",
                Some(&a),
                &favorite
            )
            .await
            .status(),
            StatusCode::OK
        );
        let response = call(&manager, Method::GET, "/api/favorites", Some(&b), "").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            json!([])
        );
        let settings = fmd_core::AppSettings {
            download_dir: "private".into(),
            ..Default::default()
        };
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/settings",
                Some(&a),
                &serde_json::to_string(&settings).unwrap()
            )
            .await
            .status(),
            StatusCode::OK
        );
        let response = call(&manager, Method::GET, "/api/settings", Some(&b), "").await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let settings: fmd_core::AppSettings = serde_json::from_slice(&body).unwrap();
        assert_eq!(settings.download_dir, "downloads");
        assert_eq!(
            call(
                &manager,
                Method::POST,
                "/api/favorites",
                Some(&b),
                &json!({"module_id":"test", "url":"url", "title":"test", "auto_download":true})
                    .to_string()
            )
            .await
            .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(&manager, Method::POST, "/api/auth/logout", Some(&a), "")
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            call(&manager, Method::GET, "/api/auth/me", Some(&b), "")
                .await
                .status(),
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn polling_does_not_extend_reservation_and_abandoned_queue_expires() {
        let manager = manager(1).await;
        let (cookie_a, _) = reserve(&manager, None).await;
        let (cookie_b, _) = reserve(&manager, None).await;
        {
            let mut state = manager.lock().await;
            state.visitors[0].last_seen = Instant::now() - IDLE_TIMEOUT + Duration::from_secs(2);
            state.visitors[1].last_seen = Instant::now() - QUEUE_TIMEOUT;
        }
        assert_eq!(
            reserve(&manager, Some(&cookie_a)).await.1["status"],
            "reserved"
        );
        let state = manager.lock().await;
        assert!(state.visitors[0].last_seen.elapsed() >= IDLE_TIMEOUT - Duration::from_secs(2));
        assert!(!state.visitors.iter().any(|v| cookie_b.ends_with(&v.key)));
    }

    #[tokio::test]
    async fn normal_user_cannot_access_admin_settings() {
        let manager = manager(1).await;
        let mut state = manager.lock().await.base.clone();
        state.shared_cache = None;
        let user = state
            .db
            .create_user("normal", Some("password"), Role::User, None, None, None)
            .await
            .unwrap();
        let token = state
            .db
            .create_session(&user.id, None, None, 1)
            .await
            .unwrap();
        let response = build_router(state, "")
            .oneshot(
                Request::builder()
                    .uri("/api/settings")
                    .header(header::COOKIE, format!("fmd_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn status_polling_does_not_keep_an_authenticated_demo_alive() {
        let manager = manager(1).await;
        let (cookie, credentials) = reserve(&manager, None).await;
        let login =
            json!({"username": credentials["username"], "password": credentials["password"]})
                .to_string();
        let response = call(
            &manager,
            Method::POST,
            "/api/auth/login",
            Some(&cookie),
            &login,
        )
        .await;
        let session = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let cookies = format!("{cookie}; {session}");
        {
            let mut state = manager.lock().await;
            state.visitors[0].last_seen = Instant::now() - IDLE_TIMEOUT + Duration::from_secs(2);
        }
        let response = call(
            &manager,
            Method::GET,
            "/api/auth/status",
            Some(&cookies),
            "",
        )
        .await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["authenticated"],
            true
        );
        {
            let mut state = manager.lock().await;
            assert!(state.visitors[0].last_seen.elapsed() >= IDLE_TIMEOUT - Duration::from_secs(2));
            state.visitors[0].last_seen = Instant::now() - IDLE_TIMEOUT;
        }
        let response = call(
            &manager,
            Method::GET,
            "/api/auth/status",
            Some(&cookies),
            "",
        )
        .await;
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap()["authenticated"],
            false
        );
    }

    #[tokio::test]
    async fn demo_routes_and_html_work_with_base_path() {
        let manager = manager(1).await;
        manager.lock().await.base_path = "/reader".into();
        let router = crate::with_base_path(
            Router::new().fallback(any(dispatch)).with_state(manager),
            "/reader",
        );
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/reader/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let html = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&html)
            .unwrap()
            .contains("const BASE_PATH = \"/reader\""));

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/reader/api/demo/reservation")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let credentials: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        let response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/reader/api/auth/login")
                    .header(header::COOKIE, cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"username": credentials["username"], "password": credentials["password"]}).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
