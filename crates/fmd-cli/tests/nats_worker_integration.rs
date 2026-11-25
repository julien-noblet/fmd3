use fmd_core::{Database, RuntimeEnvOverrides};
use fmd_engine_lua::LuaModuleEngine;
use fmd_server::{auth, build_router, AppState, CatalogSyncManager};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0x63,
    0x34, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires NATS JetStream via FMD_NATS_URL"]
async fn nats_server_worker_end_to_end_download_and_failure() {
    let nats_url = std::env::var("FMD_NATS_URL")
        .expect("FMD_NATS_URL must be set to run NATS worker integration tests");

    // Ensure clean JetStream stream before test
    let nats_client = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    let js = async_nats::jetstream::new(nats_client);
    let stream = js
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: fmd_core::STREAM_NAME.to_string(),
            subjects: vec![fmd_core::SUBJECT_TASK_ALL.to_string()],
            retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
            ..Default::default()
        })
        .await
        .expect("failed to get/create JetStream stream");
    let _ = stream.purge().await;

    // 1. Mock manga origin
    let origin = MockServer::start().await;
    let origin_uri = origin.uri();
    Mock::given(method("GET"))
        .and(path("/ch/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"<img src="{origin_uri}/img/1.png"><img src="{origin_uri}/img/2.png">"#
        )))
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/1.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/2.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/ch/fail"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!(r#"<img src="{origin_uri}/img/500.png">"#)),
        )
        .mount(&origin)
        .await;
    Mock::given(method("GET"))
        .and(path("/img/500.png"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&origin)
        .await;

    // 2. Fixture Lua module
    let modules_dir = tempfile::tempdir().unwrap();
    let downloads_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        modules_dir.path().join("NatsMock.lua"),
        format!(
            r#"
function Init()
    local m = NewWebsiteModule()
    m.ID = 'nats-mock'
    m.Name = 'NatsMock'
    m.RootURL = '{origin_uri}'
    m.Category = 'French'
    m.OnGetPageNumber = 'GetPageNumber'
end
function GetPageNumber()
    if HTTP.GET(URL) then
        for u in string.gmatch(HTTP.Document.ToString(), 'src="([^"]+)"') do
            TASK.PageLinks.Add(u)
        end
        return no_error
    end
    return net_problem
end
"#
        ),
    )
    .unwrap();

    let engine = LuaModuleEngine::new().unwrap();
    let modules = engine.scan_modules_dir(modules_dir.path()).unwrap();

    // 3. Start fmd-server connected to NATS JetStream
    let db = Database::new_in_memory().await.unwrap();
    let (ws_sender, _) = broadcast::channel(64);
    let state = AppState {
        db: db.clone(),
        shared_cache: None,
        demo_shutdown: None,
        engine: Arc::new(Mutex::new(engine)),
        modules: Arc::new(modules),
        ws_sender,
        catalog_ws_sender: None,
        sync_manager: Arc::new(CatalogSyncManager::new()),
        nats_context: Some(js),
        throttler: auth::DomainThrottler::new(),
        login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
        internal_worker_token: "nats-ti-token".to_string(),
        env_overrides: RuntimeEnvOverrides::default(),
        sfw_mode: false,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{addr}");
    let server_handle = tokio::spawn(async move {
        let _ = axum::serve(listener, build_router(state, "")).await;
    });

    // 4. Start real NATS worker
    let worker_handle = tokio::spawn(fmd_cli::run_worker(
        nats_url,
        server_url.clone(),
        1,
        modules_dir.path().to_path_buf(),
        downloads_dir.path().to_path_buf(),
        "nats-ti-token".to_string(),
    ));

    let client = reqwest::Client::new();
    let setup = client
        .post(format!("{server_url}/api/auth/setup"))
        .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup.status(), reqwest::StatusCode::OK);
    let cookie = setup
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // 5. Queue a valid chapter download -> Server -> NATS -> Worker -> CBZ + DB Completed
    let q_ok = client
        .post(format!("{server_url}/api/downloads/queue"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({
            "module_id": "nats-mock",
            "manga_title": "NatsManga",
            "format": "CBZ",
            "chapters": [{ "title": "Chapitre 1", "url": format!("{origin_uri}/ch/1") }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(q_ok.status(), reqwest::StatusCode::OK);

    let mut completed = false;
    for _ in 0..80 {
        let dls: Vec<serde_json::Value> = client
            .get(format!("{server_url}/api/downloads"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if dls
            .iter()
            .any(|d| d["chapter_title"] == "Chapitre 1" && d["status"] == "Completed")
        {
            completed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(completed, "Worker did not complete NATS download in time");

    let cbz_path = downloads_dir
        .path()
        .join("NatsManga/NatsManga - Chapitre 1.cbz");
    assert!(cbz_path.exists(), "CBZ archive must exist on disk");
    let zip = zip::ZipArchive::new(std::fs::File::open(&cbz_path).unwrap()).unwrap();
    assert_eq!(zip.len(), 3); // 001.png, 002.png, ComicInfo.xml

    // 6. Queue a failing chapter download -> Worker -> internal complete(false) -> DB Failed
    let q_fail = client
        .post(format!("{server_url}/api/downloads/queue"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({
            "module_id": "nats-mock",
            "manga_title": "NatsManga",
            "format": "CBZ",
            "chapters": [{ "title": "Chapitre Fail", "url": format!("{origin_uri}/ch/fail") }]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(q_fail.status(), reqwest::StatusCode::OK);

    let mut failed = false;
    for _ in 0..100 {
        let dls: Vec<serde_json::Value> = client
            .get(format!("{server_url}/api/downloads"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if dls
            .iter()
            .any(|d| d["chapter_title"] == "Chapitre Fail" && d["status"] == "Failed")
        {
            failed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    assert!(
        failed,
        "Worker did not report Failed status for 500 chapter"
    );

    worker_handle.abort();
    server_handle.abort();
}
