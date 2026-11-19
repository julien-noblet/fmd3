use fmd_core::{Database, RuntimeEnvOverrides};
use fmd_engine_lua::LuaModuleEngine;
use fmd_server::{auth, build_router, AppState, CatalogSyncManager};
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

async fn spawn_test_server(db: Database) -> (String, tokio::task::JoinHandle<()>) {
    let engine = Arc::new(Mutex::new(LuaModuleEngine::new().unwrap()));
    let (ws_sender, _) = broadcast::channel(32);
    let sync_manager = Arc::new(CatalogSyncManager::new());
    let state = AppState {
        db,
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
        internal_worker_token: "e2e-internal-token".to_string(),
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

async fn run_e2e_and_ui_suite(db: Database) {
    let (base_url, server_handle) = spawn_test_server(db.clone()).await;
    let client = reqwest::Client::new();

    // 1. Verify SSR HTML UI shell is served with all expected controls
    let html_res = client.get(format!("{base_url}/")).send().await.unwrap();
    assert_eq!(html_res.status(), reqwest::StatusCode::OK);
    let html = html_res.text().await.unwrap();
    for expected_id in [
        "id=\"tab-favorites\"",
        "id=\"tab-catalog\"",
        "id=\"tab-search\"",
        "id=\"tab-downloads\"",
        "id=\"tab-settings\"",
        "id=\"setting-download-dir\"",
        "id=\"setting-default-format\"",
        "id=\"setting-check-interval\"",
        "id=\"setting-catalog-resync\"",
        "id=\"btn-save-settings\"",
        "id=\"screen-login\"",
    ] {
        assert!(
            html.contains(expected_id),
            "Missing UI element {expected_id}"
        );
    }

    // 2. Unauthenticated API access must be rejected
    let unauth = client
        .get(format!("{base_url}/api/settings"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauth.status(), reqwest::StatusCode::UNAUTHORIZED);

    // 3. Initial setup creates admin and returns session cookie
    let status_json: serde_json::Value = client
        .get(format!("{base_url}/api/auth/status"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(status_json["setup_needed"], true);

    let setup_res = client
        .post(format!("{base_url}/api/auth/setup"))
        .json(&serde_json::json!({
            "username": "e2e_admin",
            "password": "E2eStrongPassword123!"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(setup_res.status(), reqwest::StatusCode::OK);
    let cookie = setup_res
        .headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("setup must set session cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // 4. Settings, Favorites, Catalog & Downloads E2E API roundtrips
    let fav_res = client
        .post(format!("{base_url}/api/favorites"))
        .header(reqwest::header::COOKIE, &cookie)
        .json(&serde_json::json!({
            "module_id": "test_mod",
            "url": "https://example.com/e2e-manga",
            "title": "E2E Manga",
            "auto_download": false,
            "download_format": "CBZ"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(fav_res.status(), reqwest::StatusCode::OK);

    let favs: Vec<serde_json::Value> = client
        .get(format!("{base_url}/api/favorites"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(favs.len(), 1);
    assert_eq!(favs[0]["title"], "E2E Manga");

    let del_res = client
        .delete(format!(
            "{base_url}/api/favorites/{}",
            url::form_urlencoded::byte_serialize(b"test_mod:https://example.com/e2e-manga")
                .collect::<String>()
        ))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(del_res.status(), reqwest::StatusCode::OK);

    // 5. Real UI control test executing the served <script> in Node against the live HTTP server
    let script_start = html.rfind("<script>").expect("missing inline script") + "<script>".len();
    let script_end = html[script_start..]
        .find("</script>")
        .expect("missing closing script")
        + script_start;
    let client_js = &html[script_start..script_end];

    let node_harness = r#"
const vm = require('vm');
let clientJs = '';
process.stdin.on('data', chunk => clientJs += chunk);
process.stdin.on('end', async () => {
    const baseUrl = process.env.E2E_BASE_URL;
    const cookie = process.env.E2E_COOKIE;
    const makeClasses = (initial = []) => {
        const set = new Set(initial);
        return {
            add: (...xs) => xs.forEach(x => set.add(x)),
            remove: (...xs) => xs.forEach(x => set.delete(x)),
            contains: x => set.has(x),
        };
    };
    const makeEl = (id, value = '', initialClasses = []) => ({
        id,
        value,
        disabled: false,
        textContent: '',
        innerHTML: '',
        options: id.startsWith('setting-') ? [
            { value: '1', textContent: '1h' },
            { value: '3', textContent: '3h' },
            { value: '6', textContent: '6h' },
            { value: '7', textContent: '7d' },
            { value: '14', textContent: '14d' },
            { value: 'CBZ', textContent: 'CBZ' },
        ] : [],
        classList: makeClasses(initialClasses),
        appendChild(opt) { this.options.push(opt); },
    });
    const elements = new Map([
        ['tab-favorites', makeEl('tab-favorites', '', [])],
        ['tab-catalog', makeEl('tab-catalog', '', ['hidden'])],
        ['tab-downloads', makeEl('tab-downloads', '', ['hidden'])],
        ['tab-settings', makeEl('tab-settings', '', ['hidden'])],
        ['nav-favorites', makeEl('nav-favorites')],
        ['nav-catalog', makeEl('nav-catalog')],
        ['nav-downloads', makeEl('nav-downloads')],
        ['nav-settings', makeEl('nav-settings')],
        ['setting-download-dir', makeEl('setting-download-dir', 'downloads')],
        ['setting-default-format', makeEl('setting-default-format', 'CBZ')],
        ['setting-naming-template', makeEl('setting-naming-template', '{manga}/{chapter}')],
        ['setting-flaresolverr-url', makeEl('setting-flaresolverr-url', '')],
        ['setting-check-interval', makeEl('setting-check-interval', '1')],
        ['setting-catalog-resync', makeEl('setting-catalog-resync', '7')],
        ['btn-save-settings', makeEl('btn-save-settings')],
        ['settings-alert', makeEl('settings-alert', '', ['hidden'])],
        ['naming-preview', makeEl('naming-preview')],
        ['favorites-list', makeEl('favorites-list')],
        ['downloads-list', makeEl('downloads-list')],
        ['catalog-list', makeEl('catalog-list')],
    ]);

    const realFetch = globalThis.fetch;
    const sandbox = {
        console,
        document: {
            getElementById: id => elements.get(id) || null,
            querySelectorAll: sel => sel === '.tab-content'
                ? [
                    elements.get('tab-favorites'),
                    elements.get('tab-catalog'),
                    elements.get('tab-downloads'),
                    elements.get('tab-settings'),
                ]
                : [],
            documentElement: {},
            createElement: () => ({ value: '', textContent: '' }),
        },
        navigator: { language: 'en' },
        LOCALES: { en: {} },
        LANGUAGES: [],
        fetch: async (url, opts = {}) => {
            const target = url.startsWith('http') ? url : `${baseUrl}${url}`;
            const headers = Object.assign({}, opts.headers || {}, { Cookie: cookie });
            return realFetch(target, Object.assign({}, opts, { headers }));
        },
        alert: msg => { throw new Error('Unexpected UI alert: ' + msg); },
        addEventListener: () => {},
        history: { replaceState: () => {}, pushState: () => {} },
        location: { protocol: 'http:', host: new URL(baseUrl).host, hash: '' },
        localStorage: { getItem: () => null, setItem: () => {} },
        sessionStorage: { getItem: () => null, setItem: () => {}, removeItem: () => {} },
        WebSocket: function() { return { close: () => {} }; },
        setTimeout: () => 0,
        clearTimeout: () => {},
        setInterval: () => 0,
        clearInterval: () => {},
        URLSearchParams,
    };
    sandbox.window = sandbox;
    try {
        vm.createContext(sandbox);
        vm.runInContext(clientJs, sandbox);

        // Switch to settings tab & load settings from live server
        sandbox.switchTab('settings', false);
        if (elements.get('tab-settings').classList.contains('hidden')) {
            throw new Error('tab-settings remained hidden after switchTab');
        }
        if (!elements.get('tab-favorites').classList.contains('hidden')) {
            throw new Error('tab-favorites was not hidden after switchTab');
        }
        await sandbox.loadSettings();
        if (elements.get('setting-download-dir').value !== 'downloads') {
            throw new Error('unexpected initial download_dir');
        }

        // Mutate UI controls and trigger saveSettings() against live server
        elements.get('setting-download-dir').value = 'e2e-downloads';
        elements.get('setting-check-interval').value = '6';
        elements.get('setting-catalog-resync').value = '14';
        await sandbox.saveSettings();

        // Reload settings into UI and verify persistence
        elements.get('setting-download-dir').value = 'stale';
        await sandbox.loadSettings();
        if (elements.get('setting-download-dir').value !== 'e2e-downloads') {
            throw new Error('saveSettings did not persist download_dir');
        }
        if (elements.get('setting-check-interval').value !== '6') {
            throw new Error('saveSettings did not persist check_interval_hours');
        }
        if (elements.get('setting-catalog-resync').value !== '14') {
            throw new Error('saveSettings did not persist catalog_resync_days');
        }

        // Switch tabs to verify favorites / catalog / downloads UI flows
        sandbox.switchTab('favorites', false);
        if (elements.get('tab-favorites').classList.contains('hidden')) {
            throw new Error('tab-favorites remained hidden');
        }
        sandbox.switchTab('catalog', false);
        if (elements.get('tab-catalog').classList.contains('hidden')) {
            throw new Error('tab-catalog remained hidden');
        }
        sandbox.switchTab('downloads', false);
        if (elements.get('tab-downloads').classList.contains('hidden')) {
            throw new Error('tab-downloads remained hidden');
        }
    } catch (err) {
        console.error(err);
        process.exitCode = 1;
    }
});
"#;

    let mut child = tokio::process::Command::new("node")
        .arg("-e")
        .arg(node_harness)
        .env("E2E_BASE_URL", &base_url)
        .env("E2E_COOKIE", &cookie)
        .stdin(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("node must be available to run UI E2E control tests");

    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(client_js.as_bytes()).await.unwrap();
        stdin.shutdown().await.unwrap();
    }
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "E2E UI control test failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify persisted settings directly via API after UI saveSettings()
    let saved_settings: fmd_core::AppSettings = client
        .get(format!("{base_url}/api/settings"))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(saved_settings.download_dir, "e2e-downloads");
    assert_eq!(saved_settings.check_interval_hours, 6);
    assert_eq!(saved_settings.catalog_resync_days, 14);

    server_handle.abort();
}

#[tokio::test]
async fn e2e_ui_controls_sqlite() {
    let db = Database::new_in_memory().await.unwrap();
    run_e2e_and_ui_suite(db).await;
}

#[tokio::test]
async fn e2e_ui_controls_postgres_if_configured() {
    let Ok(pg_url) = std::env::var("FMD_TEST_POSTGRES_URL") else {
        return;
    };
    let db = Database::new_demo_slot(Some(&pg_url)).await.unwrap();
    run_e2e_and_ui_suite(db.clone()).await;
    db.close().await;
}
