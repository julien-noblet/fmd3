use fmd_core::{Database, RuntimeEnvOverrides};
use fmd_engine_lua::LuaModuleEngine;
use fmd_server::{auth, build_router, AppState, CatalogSyncManager};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, Mutex};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn spawn_test_server(
    db: Database,
    modules: Vec<fmd_engine_lua::WebsiteModule>,
) -> (String, tokio::task::JoinHandle<()>) {
    let engine = Arc::new(Mutex::new(LuaModuleEngine::new().unwrap()));
    let (ws_sender, _) = broadcast::channel(64);
    let sync_manager = Arc::new(CatalogSyncManager::new());
    let state = AppState {
        db,
        shared_cache: None,
        demo_shutdown: None,
        engine,
        modules: Arc::new(modules),
        ws_sender,
        catalog_ws_sender: None,
        sync_manager,
        nats_context: None,
        throttler: auth::DomainThrottler::new(),
        login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
        internal_worker_token: "test-internal-token".to_string(),
        env_overrides: RuntimeEnvOverrides::default(),
        sfw_mode: false,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = build_router(state, "");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), handle)
}

async fn read_ws_text_frame(stream: &mut tokio::net::TcpStream) -> String {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).await.unwrap();
    let len_byte = header[1] & 0x7f;
    let payload_len = if len_byte == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).await.unwrap();
        u16::from_be_bytes(ext) as usize
    } else {
        len_byte as usize
    };
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).await.unwrap();
    String::from_utf8(payload).unwrap()
}

#[tokio::test]
async fn api_public_routes_security_headers_auth_and_rate_limit() {
    let db = Database::new_in_memory().await.unwrap();
    let (base_url, handle) = spawn_test_server(db, vec![]).await;
    let client = reqwest::Client::new();

    // 1. Public health, version, demo info, static assets & security headers
    let health = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    assert_eq!(
        health.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(health.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        health.headers().get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
    assert_eq!(
        health.headers().get("x-xss-protection").unwrap(),
        "1; mode=block"
    );
    assert_eq!(health.text().await.unwrap(), "OK");

    let version = client
        .get(format!("{base_url}/api/version"))
        .send()
        .await
        .unwrap();
    assert_eq!(version.status(), reqwest::StatusCode::OK);
    assert!(!version.text().await.unwrap().is_empty());

    let demo_info: serde_json::Value = client
        .get(format!("{base_url}/api/demo/info"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(demo_info["demo"], false);

    let logo = client
        .get(format!("{base_url}/logo.png"))
        .send()
        .await
        .unwrap();
    assert_eq!(logo.status(), reqwest::StatusCode::OK);
    assert_eq!(logo.headers().get("content-type").unwrap(), "image/png");

    // 2. Setup validation + one-shot enforcement + Secure cookie via X-Forwarded-Proto
    let short_pw = client
        .post(format!("{base_url}/api/auth/setup"))
        .json(&serde_json::json!({ "username": "adm", "password": "123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(short_pw.status(), reqwest::StatusCode::BAD_REQUEST);

    let setup_res = client
        .post(format!("{base_url}/api/auth/setup"))
        .header("x-forwarded-proto", "https")
        .json(&serde_json::json!({ "username": "adm", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup_res.status(), reqwest::StatusCode::OK);
    let set_cookie = setup_res
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Secure"));
    let admin_cookie = set_cookie.split(';').next().unwrap().to_string();

    // Second setup attempt must be rejected
    let second_setup = client
        .post(format!("{base_url}/api/auth/setup"))
        .json(&serde_json::json!({ "username": "adm2", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(second_setup.status(), reqwest::StatusCode::BAD_REQUEST);

    // /api/auth/me works with Cookie, Bearer, and ?token=
    let me_cookie: serde_json::Value = client
        .get(format!("{base_url}/api/auth/me"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me_cookie["user"]["username"], "adm");
    assert_eq!(me_cookie["is_api_token"], false);

    // Create & list API token
    let tok_res: serde_json::Value = client
        .post(format!("{base_url}/api/auth/tokens"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .json(&serde_json::json!({ "name": "ci", "expires_in_days": 7 }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let api_token = tok_res["token"].as_str().unwrap().to_string();
    let token_id = tok_res["info"]["id"].as_str().unwrap().to_string();

    let me_bearer: serde_json::Value = client
        .get(format!("{base_url}/api/auth/me"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {api_token}"),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me_bearer["is_api_token"], true);

    let me_query: serde_json::Value = client
        .get(format!("{base_url}/api/auth/me?token={api_token}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(me_query["user"]["username"], "adm");

    // Delete API token -> subsequent Bearer rejected
    let del_tok = client
        .delete(format!("{base_url}/api/auth/tokens/{token_id}"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(del_tok.status(), reqwest::StatusCode::OK);
    let rejected = client
        .get(format!("{base_url}/api/auth/me"))
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {api_token}"),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), reqwest::StatusCode::UNAUTHORIZED);

    // Logout invalidates session cookie
    let logout_res = client
        .post(format!("{base_url}/api/auth/logout"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(logout_res.status(), reqwest::StatusCode::OK);
    let post_logout = client
        .get(format!("{base_url}/api/auth/me"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(post_logout.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 3. Login brute-force lockout after 5 failures
    for _ in 0..5 {
        let fail = client
            .post(format!("{base_url}/api/auth/login"))
            .header("x-forwarded-for", "198.51.100.42")
            .json(&serde_json::json!({ "username": "adm", "password": "wrong" }))
            .send()
            .await
            .unwrap();
        assert_eq!(fail.status(), reqwest::StatusCode::UNAUTHORIZED);
    }
    let locked = client
        .post(format!("{base_url}/api/auth/login"))
        .header("x-forwarded-for", "198.51.100.42")
        .json(&serde_json::json!({ "username": "adm", "password": "StrongPassword123!" }))
        .send()
        .await
        .unwrap();
    assert_eq!(locked.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);

    handle.abort();
}

#[tokio::test]
async fn api_rbac_admin_users_permissions_and_cache_policies() {
    let db = Database::new_in_memory().await.unwrap();
    let (base_url, handle) = spawn_test_server(db, vec![]).await;
    let client = reqwest::Client::new();

    // Setup admin
    let setup = client
        .post(format!("{base_url}/api/auth/setup"))
        .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
    let admin_cookie = setup
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // Admin creates restricted standard user
    let create_user_res = client
        .post(format!("{base_url}/api/admin/users"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .json(&serde_json::json!({
            "username": "restricted",
            "password": "UserPassword123!",
            "role": "user",
            "permissions": {
                "user_id": "",
                "can_download": false,
                "can_manage_favorites": false,
                "can_trigger_catalog_sync": false,
                "can_force_refresh": false,
                "can_view_modules": false
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(create_user_res.status(), reqwest::StatusCode::OK);
    let created_json: serde_json::Value = create_user_res.json().await.unwrap();
    let user_id = created_json["user"]["id"].as_str().unwrap().to_string();

    // List users & verify admin self-delete is blocked
    let users_list: Vec<serde_json::Value> = client
        .get(format!("{base_url}/api/admin/users"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(users_list.len(), 2);
    let admin_id = users_list
        .iter()
        .find(|u| u["user"]["username"] == "admin")
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let self_del = client
        .delete(format!("{base_url}/api/admin/users/{admin_id}"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(self_del.status(), reqwest::StatusCode::BAD_REQUEST);

    // Login as restricted user
    let user_login = client
        .post(format!("{base_url}/api/auth/login"))
        .json(&serde_json::json!({ "username": "restricted", "password": "UserPassword123!" }))
        .send()
        .await
        .unwrap();
    let user_cookie = user_login
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // Non-admin blocked from admin routes & settings
    for (method_str, path_str) in [
        ("GET", "/api/admin/users"),
        ("GET", "/api/admin/cache/policies"),
        ("GET", "/api/settings"),
        ("POST", "/api/settings"),
    ] {
        let req = match method_str {
            "POST" => client
                .post(format!("{base_url}{path_str}"))
                .json(&fmd_core::AppSettings::default()),
            _ => client.get(format!("{base_url}{path_str}")),
        };
        let res = req
            .header(reqwest::header::COOKIE, &user_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            reqwest::StatusCode::FORBIDDEN,
            "{method_str} {path_str}"
        );
    }

    // Authenticated user can list modules; granular permissions enforced on favorites, downloads, catalog sync
    assert_eq!(
        client
            .get(format!("{base_url}/api/modules"))
            .header(reqwest::header::COOKIE, &user_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        client
            .post(format!("{base_url}/api/favorites"))
            .header(reqwest::header::COOKIE, &user_cookie)
            .json(&serde_json::json!({"module_id":"m","url":"u","title":"t"}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{base_url}/api/downloads/queue"))
            .header(reqwest::header::COOKIE, &user_cookie)
            .json(&serde_json::json!({"module_id":"m","manga_title":"t","chapters":[]}))
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    assert_eq!(
        client
            .post(format!("{base_url}/api/catalog/sync/any"))
            .header(reqwest::header::COOKIE, &user_cookie)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );

    // Admin cache policy CRUD + user update + user delete
    let policy_res = client
        .post(format!("{base_url}/api/admin/cache/policies"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .json(&serde_json::json!({
            "module_id": "test-mod",
            "ttl_value": 12,
            "ttl_unit": "hours",
            "min_delay_ms": 500
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(policy_res.status(), reqwest::StatusCode::OK);
    let clear_cache = client
        .delete(format!("{base_url}/api/admin/cache/test-mod%3A1"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(clear_cache.status(), reqwest::StatusCode::OK);

    let upd_user = client
        .put(format!("{base_url}/api/admin/users/{user_id}"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .json(&serde_json::json!({ "is_active": false }))
        .send()
        .await
        .unwrap();
    assert_eq!(upd_user.status(), reqwest::StatusCode::OK);
    let del_user = client
        .delete(format!("{base_url}/api/admin/users/{user_id}"))
        .header(reqwest::header::COOKIE, &admin_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(del_user.status(), reqwest::StatusCode::OK);

    handle.abort();
}

#[tokio::test]
async fn api_favorites_downloads_merger_catalog_internal_callbacks_and_websocket() {
    let mock_site = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/dir/0"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<a href="/manga/alpha">Alpha Manga</a><a href="/manga/beta">Beta Manga</a>"#,
        ))
        .mount(&mock_site)
        .await;

    let tmp_dir = tempfile::tempdir().unwrap();
    let lua_path = tmp_dir.path().join("SyncMod.lua");
    std::fs::write(
        &lua_path,
        format!(
            r#"
function Init()
    local m = NewWebsiteModule()
    m.ID = 'sync-mod'
    m.Name = 'SyncMod'
    m.RootURL = '{base}'
    m.Category = 'English'
    m.OnGetNameAndLink = 'GetNameAndLink'
    m.TotalDirectory = 1
end
function GetNameAndLink()
    if HTTP.GET(MODULE.RootURL .. '/dir/' .. URL) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        x.XPathHREFAll('//a', LINKS, NAMES)
        return no_error
    end
    return net_problem
end
"#,
            base = mock_site.uri()
        ),
    )
    .unwrap();

    let engine = LuaModuleEngine::new().unwrap();
    let modules = engine.scan_modules_dir(tmp_dir.path()).unwrap();
    assert_eq!(modules.len(), 1);

    let db = Database::new_in_memory().await.unwrap();
    let (base_url, handle) = spawn_test_server(db.clone(), modules).await;
    let client = reqwest::Client::new();

    // Setup admin + create API token for WS
    let setup = client
        .post(format!("{base_url}/api/auth/setup"))
        .json(&serde_json::json!({ "username": "admin", "password": "AdminPassword123!" }))
        .send()
        .await
        .unwrap();
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

    let tok_json: serde_json::Value = client
        .post(format!("{base_url}/api/auth/tokens"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({ "name": "ws-test" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let api_token = tok_json["token"].as_str().unwrap().to_string();

    // 1. WebSocket requires auth, then delivers live events on internal callbacks
    let unauth_ws = client
        .get(format!("{base_url}/api/ws"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth_ws.status(), reqwest::StatusCode::UNAUTHORIZED);

    let host_port = base_url.strip_prefix("http://").unwrap();
    let mut ws_stream = tokio::net::TcpStream::connect(host_port).await.unwrap();
    let upgrade_req = format!(
        "GET /api/ws?token={api_token} HTTP/1.1\r\n\
         Host: {host_port}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n"
    );
    ws_stream.write_all(upgrade_req.as_bytes()).await.unwrap();

    // Read HTTP 101 headers
    let mut resp_buf = Vec::new();
    let mut byte = [0u8; 1];
    while !resp_buf.ends_with(b"\r\n\r\n") {
        ws_stream.read_exact(&mut byte).await.unwrap();
        resp_buf.push(byte[0]);
    }
    let resp_head = String::from_utf8_lossy(&resp_buf);
    assert!(resp_head.contains("101 Switching Protocols"), "{resp_head}");

    let welcome = read_ws_text_frame(&mut ws_stream).await;
    assert!(welcome.contains("\"connected\""), "{welcome}");

    // 2. Queue download -> internal progress & complete callbacks -> WS live frame + DB update
    let q_res: serde_json::Value = client
        .post(format!("{base_url}/api/downloads/queue"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({
            "module_id": "sync-mod",
            "manga_title": "Alpha Manga",
            "format": "CBZ",
            "chapters": [{ "title": "Ch 1", "url": "/ch/1" }]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(q_res["queued"], 1);

    let dls: Vec<serde_json::Value> = client
        .get(format!("{base_url}/api/downloads"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let dl_id = dls[0]["id"].as_str().unwrap().to_string();

    // Reject internal callback with wrong token
    let bad_cb = client
        .post(format!("{base_url}/api/internal/tasks/progress"))
        .header("X-FMD-Internal-Token", "wrong")
        .json(&serde_json::json!({
            "task_id": dl_id,
            "status": "Downloading",
            "downloaded_pages": 1,
            "total_pages": 2,
            "message": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_cb.status(), reqwest::StatusCode::FORBIDDEN);

    // Valid progress callback triggers WS broadcast
    let ok_cb = client
        .post(format!("{base_url}/api/internal/tasks/progress"))
        .header("X-FMD-Internal-Token", "test-internal-token")
        .json(&serde_json::json!({
            "task_id": dl_id,
            "status": "Downloading",
            "downloaded_pages": 1,
            "total_pages": 2,
            "message": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ok_cb.status(), reqwest::StatusCode::OK);

    let ws_queued = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read_ws_text_frame(&mut ws_stream),
    )
    .await
    .unwrap();
    assert!(ws_queued.contains("download_queued"), "{ws_queued}");

    let ws_progress = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        read_ws_text_frame(&mut ws_stream),
    )
    .await
    .unwrap();
    assert!(ws_progress.contains("download_progress"), "{ws_progress}");

    // Complete failure -> retry -> delete
    let fail_cb = client
        .post(format!("{base_url}/api/internal/tasks/complete"))
        .header("Authorization", "Bearer test-internal-token")
        .json(&serde_json::json!({
            "task_id": dl_id,
            "success": false,
            "error_message": "boom",
            "saved_path": null
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fail_cb.status(), reqwest::StatusCode::OK);

    let retry_res = client
        .post(format!("{base_url}/api/downloads/{dl_id}/retry"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(retry_res.status(), reqwest::StatusCode::OK);

    let del_dl = client
        .delete(format!("{base_url}/api/downloads/{dl_id}"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(del_dl.status(), reqwest::StatusCode::OK);

    // 3. Favorites CRUD + PATCH settings + chapters
    let add_fav = client
        .post(format!("{base_url}/api/favorites"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({
            "module_id": "sync-mod",
            "url": "https://example.com/alpha",
            "title": "Alpha Manga",
            "auto_download": true,
            "download_format": "CBZ"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_fav.status(), reqwest::StatusCode::OK);

    let fav_id = url::form_urlencoded::byte_serialize(b"sync-mod:https://example.com/alpha")
        .collect::<String>();
    let patch_fav = client
        .patch(format!("{base_url}/api/favorites/{fav_id}"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({ "enabled": false, "auto_download": false, "download_format": "PDF" }))
        .send()
        .await
        .unwrap();
    assert_eq!(patch_fav.status(), reqwest::StatusCode::OK);

    let fav_chaps = client
        .get(format!("{base_url}/api/favorites/{fav_id}/chapters"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(fav_chaps.status(), reqwest::StatusCode::OK);

    let del_fav = client
        .delete(format!("{base_url}/api/favorites/{fav_id}"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(del_fav.status(), reqwest::StatusCode::OK);

    // 4. Catalog sync -> search -> cancel
    let sync_start = client
        .post(format!("{base_url}/api/catalog/sync/sync-mod"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(sync_start.status(), reqwest::StatusCode::OK);

    // Poll briefly until Alpha Manga appears in catalog search
    let mut found_alpha = false;
    for _ in 0..40 {
        let search: serde_json::Value = client
            .get(format!("{base_url}/api/catalog/search?q=Alpha"))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if search["total"].as_i64().unwrap_or(0) >= 1 {
            found_alpha = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(found_alpha, "Catalog sync did not populate Alpha Manga");

    let cat_status: serde_json::Value = client
        .get(format!("{base_url}/api/catalog/status"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(cat_status["total_catalog_items"].as_i64().unwrap_or(0) >= 2);

    let cancel = client
        .post(format!("{base_url}/api/catalog/sync/cancel"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(cancel.status(), reqwest::StatusCode::OK);

    handle.abort();
}
