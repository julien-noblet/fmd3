use fmd_core::{Database, RuntimeEnvOverrides};
use fmd_engine_lua::LuaModuleEngine;
use fmd_server::{auth, build_router, AppState, CatalogSyncManager};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};

fn fmd_cmd() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fmd"));
    // Ensure devenv's FMD_DATABASE_URL does not override explicit --db SQLite paths
    cmd.env_remove("FMD_DATABASE_URL");
    cmd
}

#[test]
fn cli_subprocess_help_version_config_auth_modules_and_import_legacy() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("cli_test.db");

    // 1. --help and --version
    let help = fmd_cmd().arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("serve"));

    let ver = fmd_cmd().arg("--version").output().unwrap();
    assert!(ver.status.success());
    assert!(String::from_utf8_lossy(&ver.stdout).contains(env!("CARGO_PKG_VERSION")));

    // 2. auth create-user -> list-users -> create-token
    let create_admin = fmd_cmd()
        .args([
            "auth",
            "create-user",
            "-u",
            "cli_admin",
            "-p",
            "CliPass123!",
            "-r",
            "admin",
            "--db",
        ])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(
        create_admin.status.success(),
        "{}",
        String::from_utf8_lossy(&create_admin.stderr)
    );

    let list_users = fmd_cmd()
        .args(["auth", "list-users", "--db"])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(list_users.status.success());
    let users_out = String::from_utf8_lossy(&list_users.stdout);
    assert!(users_out.contains("cli_admin [admin]"));

    let create_token = fmd_cmd()
        .args([
            "auth",
            "create-token",
            "-u",
            "cli_admin",
            "-n",
            "ci-e2e",
            "--db",
        ])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(create_token.status.success());
    let tok_out = String::from_utf8_lossy(&create_token.stdout);
    assert!(tok_out.contains("Authorization: Bearer fmd_"));

    // 3. config set -> get -> list
    let set_cfg = fmd_cmd()
        .args(["config", "--set", "download_dir=/tmp/fmd-e2e-dl", "--db"])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(set_cfg.status.success());

    let get_cfg = fmd_cmd()
        .args(["config", "--get", "download_dir", "--db"])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(get_cfg.status.success());
    assert!(String::from_utf8_lossy(&get_cfg.stdout).contains("/tmp/fmd-e2e-dl"));

    let list_cfg = fmd_cmd()
        .args(["config", "--list", "--db"])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(list_cfg.status.success());
    assert!(String::from_utf8_lossy(&list_cfg.stdout).contains("download_dir"));

    // 4. modules --dir <dir> --check <name>
    let mod_dir = tmp.path().join("mods");
    std::fs::create_dir_all(&mod_dir).unwrap();
    std::fs::write(
        mod_dir.join("FixtureCli.lua"),
        r#"
function Init()
    local m = NewWebsiteModule()
    m.ID = 'fixture-cli'
    m.Name = 'FixtureCli'
    m.RootURL = 'https://fixture.cli'
    m.Category = 'English'
end
"#,
    )
    .unwrap();
    let mod_check = fmd_cmd()
        .args(["modules", "--dir"])
        .arg(&mod_dir)
        .args(["--check", "FixtureCli"])
        .output()
        .unwrap();
    assert!(mod_check.status.success());
    let mod_out = String::from_utf8_lossy(&mod_check.stdout);
    assert!(mod_out.contains("FixtureCli"));
    assert!(mod_out.contains("fixture-cli"));

    // 5. import-legacy -> favorites --list
    let legacy_path = tmp.path().join("legacy.db");
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let url = format!("sqlite://{}?mode=rwc", legacy_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE favorites (
                id TEXT, "order" INTEGER, enabled BOOLEAN, moduleid TEXT,
                link TEXT, title TEXT, status TEXT, currentchapter TEXT,
                saveto TEXT, dateadded TEXT, datelastchecked TEXT, datelastupdated TEXT
            );
            CREATE TABLE downloadedchapters (id TEXT, chapters TEXT);
            INSERT INTO favorites VALUES (
                'cli-leg-1', 1, 1, 'fixture-cli', 'https://fixture.cli/m1',
                'Legacy CLI Manga', 'ongoing', 'Ch 1', '/tmp/s', '', '', ''
            );
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    });

    let imp = fmd_cmd()
        .arg("import-legacy")
        .arg(&legacy_path)
        .arg("--target-db")
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(
        imp.status.success(),
        "{}",
        String::from_utf8_lossy(&imp.stderr)
    );
    assert!(String::from_utf8_lossy(&imp.stdout).contains("1 favorites/series"));

    let favs = fmd_cmd()
        .args(["favorites", "--list", "--db"])
        .arg(&db_path)
        .output()
        .unwrap();
    assert!(favs.status.success());
    assert!(String::from_utf8_lossy(&favs.stdout).contains("Legacy CLI Manga"));
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_subprocess_auth_status_and_login_against_live_server() {
    let db = Database::new_in_memory().await.unwrap();
    db.create_initial_admin("live_admin", "LivePassword123!")
        .await
        .unwrap();

    let (ws_sender, _) = broadcast::channel(16);
    let state = AppState {
        db,
        shared_cache: None,
        demo_shutdown: None,
        engine: Arc::new(Mutex::new(LuaModuleEngine::new().unwrap())),
        modules: Arc::new(vec![]),
        ws_sender,
        catalog_ws_sender: None,
        sync_manager: Arc::new(CatalogSyncManager::new()),
        nats_context: None,
        throttler: auth::DomainThrottler::new(),
        login_rate_limiter: Arc::new(auth::LoginRateLimiter::new()),
        internal_worker_token: "cli-e2e-token".to_string(),
        env_overrides: RuntimeEnvOverrides::default(),
        sfw_mode: false,
    };

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server_url = format!("http://{addr}");
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, build_router(state, "")).await;
    });

    let server_url_status = server_url.clone();
    let status_out = tokio::task::spawn_blocking(move || {
        fmd_cmd()
            .args(["auth", "status", "--server", &server_url_status])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(status_out.status.success());
    assert!(String::from_utf8_lossy(&status_out.stdout).contains("Setup needed  : false"));

    let server_url_login = server_url.clone();
    let login_out = tokio::task::spawn_blocking(move || {
        fmd_cmd()
            .args([
                "auth",
                "login",
                "--server",
                &server_url_login,
                "-u",
                "live_admin",
                "-p",
                "LivePassword123!",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(login_out.status.success());
    let login_stdout = String::from_utf8_lossy(&login_out.stdout);
    assert!(login_stdout.contains("Authentication successful!"));
    assert!(login_stdout.contains("live_admin"));

    handle.abort();
}
