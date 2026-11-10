use clap::{Parser, Subcommand};
use fmd_core::Database;
use fmd_engine_lua::LuaModuleEngine;
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "fmd", author, version, about = "Free Manga Downloader 2 - Linux Headless Daemon & Web Server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the FMD3 San (三) headless daemon and Web server
    Serve {
        /// Bind address
        #[arg(short, long, default_value = "0.0.0.0:8088")]
        bind: SocketAddr,
    },
    /// Manage manga modules
    Modules {
        /// Directory containing modules
        #[arg(short, long, default_value = "lua/modules")]
        dir: PathBuf,
        /// Check/diagnose a specific module by name
        #[arg(short, long)]
        check: Option<String>,
        /// Test URL for manga to verify with --check
        #[arg(short, long)]
        url: Option<String>,
    },
    /// Import legacy FMD2 SQLite database (userdata.db / favorites / downloaded chapters)
    ImportLegacy {
        /// Path to legacy FMD2 SQLite database file
        path: PathBuf,
        /// Path to target modern FMD3 SQLite database
        #[arg(short, long, default_value = "data/fmd.db")]
        target_db: PathBuf,
    },
    /// Search manga catalog (FTS5) or synchronize a module
    Catalog {
        /// Search query (FTS5 title search)
        #[arg(short, long)]
        search: Option<String>,
        /// Filter by module name / ID
        #[arg(short, long)]
        module: Option<String>,
        /// Synchronize a specific module's catalog into local SQLite
        #[arg(long)]
        sync: Option<String>,
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
        /// Modules directory
        #[arg(long, default_value = "lua/modules")]
        dir: PathBuf,
        /// Maximum number of results to display
        #[arg(short, long, default_value = "25")]
        limit: i64,
    },
    /// Manage favorites, check for new chapters and track downloads
    Favorites {
        /// List all tracked favorites with stats
        #[arg(short, long)]
        list: bool,
        /// Check for new chapters on favorites
        #[arg(short, long)]
        check: bool,
        /// Specific manga ID or title to check
        #[arg(long)]
        id: Option<String>,
        /// Automatically queue downloads for newly detected chapters
        #[arg(long)]
        auto_download: bool,
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
        /// Modules directory
        #[arg(long, default_value = "lua/modules")]
        dir: PathBuf,
    },
    /// View and update settings
    Config {
        /// Get a setting value by key
        #[arg(short, long)]
        get: Option<String>,
        /// Set a setting value (format: KEY=VALUE)
        #[arg(short, long)]
        set: Option<String>,
        /// List all current settings
        #[arg(short, long)]
        list: bool,
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
    },
    /// Start a standalone background worker connected to NATS JetStream
    Worker {
        /// NATS server URL
        #[arg(long, env = "FMD_NATS_URL", default_value = "nats://127.0.0.1:4222")]
        nats_url: String,
        /// FMD3 Server API URL for progress reporting & DB updates
        #[arg(long, env = "FMD_SERVER_URL", default_value = "http://127.0.0.1:8088")]
        server_url: String,
        /// Concurrency level (number of concurrent tasks)
        #[arg(short, long, default_value = "2")]
        concurrency: usize,
        /// Directory containing Lua modules
        #[arg(short, long, default_value = "lua/modules")]
        dir: PathBuf,
        /// Directory for downloaded archives
        #[arg(long, default_value = "downloads")]
        downloads_dir: PathBuf,
        /// Internal worker authentication token
        #[arg(long, env = "FMD_INTERNAL_TOKEN", default_value = "fmd_internal_worker_secret_key_san")]
        internal_token: String,
    },
    /// Manage authentication, users, and API tokens
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
}

#[derive(Subcommand, Debug)]
enum AuthCommands {
    /// Login to FMD3 server and obtain session token
    Login {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:8088")]
        server: String,
        /// Username
        #[arg(short, long)]
        username: String,
        /// Password
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Check server authentication and setup status
    Status {
        /// Server URL
        #[arg(long, default_value = "http://127.0.0.1:8088")]
        server: String,
        /// API or session token
        #[arg(long)]
        token: Option<String>,
    },
    /// Generate an API token for a user (direct database access)
    CreateToken {
        /// Username
        #[arg(short, long)]
        username: String,
        /// Token name
        #[arg(short, long, default_value = "cli-token")]
        name: String,
        /// Optional expiration in days
        #[arg(short, long)]
        expires_days: Option<i64>,
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
    },
    /// List all registered users (direct database access)
    ListUsers {
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
    },
    /// Create a new user (direct database access)
    CreateUser {
        /// Username
        #[arg(short, long)]
        username: String,
        /// Password
        #[arg(short, long)]
        password: String,
        /// Role: admin or user
        #[arg(short, long, default_value = "user")]
        role: String,
        /// Path to modern FMD3 SQLite database
        #[arg(long, default_value = "data/fmd.db")]
        db: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { bind } => {
            fmd_server::run_server(bind).await?;
        }
        Commands::Worker {
            nats_url,
            server_url,
            concurrency,
            dir,
            downloads_dir,
            internal_token,
        } => {
            run_worker(
                nats_url,
                server_url,
                concurrency,
                dir,
                downloads_dir,
                internal_token,
            )
            .await?;
        }
        Commands::Modules { dir, check, url } => {
            let engine = LuaModuleEngine::new()?;
            if let Some(target_name) = check {
                println!("Searching for module '{}' in {:?}...", target_name, dir);
                let modules = engine.scan_modules_dir(&dir)?;
                let module = modules.into_iter().find(|m| {
                    m.name.eq_ignore_ascii_case(&target_name)
                        || m.id.eq_ignore_ascii_case(&target_name)
                });

                match module {
                    Some(m) => {
                        println!("\n=== Module Health Diagnostic ===");
                        println!("Name:       {}", m.name);
                        println!("ID:         {}", m.id);
                        println!("Category:   {}", m.category);
                        println!("Root URL:   {}", m.root_url);
                        println!("File Path:  {}", m.file_path);
                        println!("OnGetInfo:  {:?}", m.on_get_info);
                        println!("OnGetName:  {:?}", m.on_get_name_and_link);
                        println!("OnGetPages: {:?}", m.on_get_page_number);
                        println!("OnBeforeDl: {:?}", m.on_before_download_image);
                        println!("OnDownload: {:?}", m.on_download_image);

                        if let Some(test_url) = url {
                            println!("\nTesting Manga Info extraction with URL: {}", test_url);
                            match engine.get_manga_info(&m.file_path, &test_url) {
                                Ok(info) => {
                                    println!("✓ Title:    {}", info.title);
                                    println!("✓ Status:   {}", info.status);
                                    println!("✓ Genres:   {:?}", info.genres);
                                    println!("✓ Chapters: {} discovered", info.chapters.len());

                                    if let Some(first_ch) = info.chapters.first() {
                                        println!("\nTesting Chapter Pages extraction on chapter: '{}' ({})", first_ch.title, first_ch.url);
                                        match engine.get_chapter_pages(&m.file_path, &first_ch.url) {
                                            Ok(pages) => {
                                                println!("✓ Pages:    {} detected", pages.len());
                                                if let Some(first_page) = pages.first() {
                                                    println!("✓ Page 1:   {}", first_page);

                                                    let headers = engine.prepare_download_image(&m.file_path, first_page, 0).unwrap_or_default();
                                                    println!("✓ Headers:  {:?}", headers);

                                                    let http = engine.http_client();
                                                    let custom_bytes = engine.download_custom_image(&m.file_path, first_page, 0).ok().flatten();
                                                    let fetch_res = match custom_bytes {
                                                        Some(b) => Ok(b),
                                                        None => http.get_bytes_with_headers(first_page, &headers).await,
                                                    };

                                                    match fetch_res {
                                                        Ok(b) => println!("✓ Download: Success ({} bytes received)", b.len()),
                                                        Err(e) => println!("✗ Download failed: {}", e),
                                                    }
                                                }
                                            }
                                            Err(e) => println!("✗ get_chapter_pages failed: {}", e),
                                        }
                                    }
                                }
                                Err(e) => println!("✗ get_manga_info failed: {}", e),
                            }
                        } else {
                            println!("\n(Tip: Pass --url <MANGA_URL> to test live chapter parsing and image download)");
                        }
                    }
                    None => {
                        let wasm_path = if target_name.ends_with(".wasm") {
                            dir.join(&target_name)
                        } else {
                            dir.join(format!("{}.wasm", target_name))
                        };
                        if wasm_path.exists() {
                            let wasm_engine = Arc::new(fmd_engine_wasm::WasmEngine::new()?);
                            let wasm_mod = fmd_engine_wasm::WasmMangaModule::load_from_file(wasm_engine, &wasm_path)?;
                            println!("\n=== WASM Module Diagnostic ===");
                            println!("Name:       {}", wasm_mod.name());
                            println!("ID:         {}", wasm_mod.id());
                            println!("Version:    {}", wasm_mod.version());
                            println!("Root URL:   {}", wasm_mod.root_url());
                            println!("File Path:  {}", wasm_path.display());

                            if let Some(test_url) = url {
                                println!("\nTesting Manga Info extraction with URL: {}", test_url);
                                match wasm_mod.get_manga_info(&test_url) {
                                    Ok(info) => {
                                        println!("✓ Title:    {}", info.title);
                                        println!("✓ Status:   {:?}", info.status);
                                        println!("✓ Genres:   {:?}", info.genres);
                                        println!("✓ Chapters: {} discovered", info.chapters.len());
                                    }
                                    Err(e) => eprintln!("✗ Failed to extract info: {}", e),
                                }
                            }
                        } else {
                            eprintln!("Error: Module '{}' not found in {:?}", target_name, dir);
                            std::process::exit(1);
                        }
                    }
                }
            } else {
                println!("Scanning modules in {:?}...", dir);
                let modules = engine.scan_modules_dir(&dir)?;
                println!("Found {} active modules:", modules.len());
                for m in modules.iter().take(20) {
                    println!(" - [{}] {} ({}) -> {}", m.category, m.name, m.root_url, m.id);
                }
                if modules.len() > 20 {
                    println!(" ... and {} more modules.", modules.len() - 20);
                }
            }
        }
        Commands::ImportLegacy { path, target_db } => {
            println!("Connecting to modern database at {:?}...", target_db);
            let db = Database::new(target_db).await?;
            println!("Importing legacy FMD2 database from {:?}...", path);
            let count = db.import_legacy_db(path).await?;
            println!("Successfully imported {} favorites/series!", count);
        }
        Commands::Catalog { search, module, sync, db, dir, limit } => {
            let database = Database::new(db).await?;

            if let Some(mod_target) = sync {
                let engine = LuaModuleEngine::new()?;
                println!("Searching for module '{}' in {:?}...", mod_target, dir);
                let modules = engine.scan_modules_dir(&dir)?;
                let m = modules.into_iter().find(|m| {
                    m.name.eq_ignore_ascii_case(&mod_target)
                        || m.id.eq_ignore_ascii_case(&mod_target)
                }).ok_or_else(|| anyhow::anyhow!("Module '{}' not found", mod_target))?;

                println!("Starting catalog sync for [{}] {} (Root: {})...", m.category, m.name, m.root_url);
                database.set_module_sync_status(&m.id, "Syncing", None, None).await?;

                let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let db_clone = database.clone();
                let m_id = m.id.clone();
                let file_path = m.file_path.clone();

                let total = tokio::task::spawn_blocking(move || {
                    let handle = tokio::runtime::Handle::current();
                    engine.update_catalog(&file_path, cancel_token, |entries, progress| {
                        let items: Vec<(String, String)> = entries.into_iter().map(|e| (e.title, e.link)).collect();
                        let count = items.len();
                        
                        handle.block_on(async {
                            let _ = db_clone.upsert_catalog_items(&m_id, &items).await;
                        });

                        let p_str = progress.total_pages.map(|tp| format!("{}/{}", progress.current_page + 1, tp)).unwrap_or_else(|| format!("{}", progress.current_page + 1));
                        println!("  Page {} (Dir {}/{}): +{} mangas (Total: {})", p_str, progress.directory_index + 1, progress.total_directories, count, progress.items_found);
                        Ok(())
                    })
                }).await??;

                database.set_module_sync_status(&m.id, "Completed", None, Some(total as i64)).await?;
                println!("✓ Successfully synchronized {} mangas from module '{}'!", total, m.name);
            } else {
                let query = search.unwrap_or_default();
                let mod_id = if let Some(ref m_name) = module {
                    let engine = LuaModuleEngine::new().ok();
                    let modules = engine.and_then(|e| e.scan_modules_dir(&dir).ok()).unwrap_or_default();
                    modules.iter().find(|m| m.name.eq_ignore_ascii_case(m_name) || m.id.eq_ignore_ascii_case(m_name)).map(|m| m.id.clone()).or_else(|| Some(m_name.clone()))
                } else {
                    None
                };
                let (items, total) = database.search_catalog(&query, mod_id.as_deref(), limit, 0).await?;
                println!("=== Catalog Search Results ({} total match(es)) ===", total);
                if items.is_empty() {
                    println!("No mangas found. Use `fmd catalog --sync <MODULE>` to populate local catalog.");
                } else {
                    for (idx, item) in items.iter().enumerate() {
                        println!(" {:2}. [{}] {} -> {}", idx + 1, item.module_id, item.title, item.link);
                    }
                }
            }
        }
        Commands::Favorites { list: _, check, id, auto_download, db, dir } => {
            let database = Database::new(db).await?;
            let favorites = database.list_favorites_with_stats().await?;

            if check {
                let engine = LuaModuleEngine::new()?;
                let modules = engine.scan_modules_dir(&dir)?;

                let target_favs: Vec<_> = favorites
                    .into_iter()
                    .filter(|f| {
                        if let Some(ref target_id) = id {
                            f.manga_id == *target_id
                                || f.title.to_lowercase().contains(&target_id.to_lowercase())
                        } else {
                            f.enabled
                        }
                    })
                    .collect();

                println!("Checking {} favorite(s) for new chapters...", target_favs.len());
                let mut total_new = 0;

                for fav in target_favs {
                    let module = modules
                        .iter()
                        .find(|m| m.id == fav.module_id || m.name.eq_ignore_ascii_case(&fav.module_id));
                    let m = match module {
                        Some(m) => m,
                        None => {
                            eprintln!(
                                "Warning: module '{}' for manga '{}' not found in {:?}",
                                fav.module_id, fav.title, dir
                            );
                            continue;
                        }
                    };

                    print!("- [{}] {} ... ", fav.module_id, fav.title);
                    match engine.get_manga_info(&m.file_path, &fav.original_url) {
                        Ok(info) => {
                            let chaps: Vec<fmd_core::ChapterItem> = info
                                .chapters
                                .into_iter()
                                .map(|c| fmd_core::ChapterItem {
                                    title: c.title,
                                    url: c.url,
                                })
                                .collect();

                            match database.check_favorite_new_chapters(&fav.manga_id, &chaps).await {
                                Ok(new_chaps) => {
                                    if new_chaps.is_empty() {
                                        println!("Up to date ({} chapters)", chaps.len());
                                    } else {
                                        println!("🎉 {} NEW chapter(s) found!", new_chaps.len());
                                        total_new += new_chaps.len();
                                        for ch in &new_chaps {
                                            println!("    + {}", ch.title);
                                        }

                                        let should_dl = auto_download || fav.auto_download;
                                        if should_dl {
                                            println!("    -> Queuing for download ({})", fav.download_format);
                                            for ch in new_chaps {
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
                                                .execute(database.pool())
                                                .await;
                                            }
                                        }
                                    }
                                }
                                Err(e) => println!("Database error: {}", e),
                            }
                        }
                        Err(e) => println!("Error fetching info: {}", e),
                    }
                }
                println!(
                    "\nCheck completed: {} new chapter(s) discovered in total.",
                    total_new
                );
            } else {
                // Default: list favorites
                println!("=== Tracked Favorites & Library ({}) ===", favorites.len());
                if favorites.is_empty() {
                    println!("No favorites found. Add mangas using Web UI or API.");
                } else {
                    println!(
                        "{:<4} | {:<16} | {:<28} | {:<12} | {:<8} | {:<6}",
                        "No", "Module", "Manga Title", "Chapters", "Auto-DL", "Format"
                    );
                    println!(
                        "{:-<4}-+-{:-<16}-+-{:-<28}-+-{:-<12}-+-{:-<8}-+-{:-<6}",
                        "", "", "", "", "", ""
                    );
                    for (idx, f) in favorites.iter().enumerate() {
                        let dl_stat = format!("{}/{}", f.downloaded_chapters_count, f.total_chapters);
                        let auto_dl = if f.auto_download { "ON" } else { "OFF" };
                        let title = if f.title.len() > 28 {
                            format!("{}...", &f.title[..25])
                        } else {
                            f.title.clone()
                        };
                        println!(
                            "{:<4} | {:<16} | {:<28} | {:<12} | {:<8} | {:<6}",
                            idx + 1,
                            f.module_id,
                            title,
                            dl_stat,
                            auto_dl,
                            f.download_format
                        );
                    }
                }
            }
        }
        Commands::Config { get, set, list: _, db } => {
            let database = Database::new(db).await?;
            if let Some(key) = get {
                let val = database.get_setting(&key).await?;
                println!("{}: {}", key, val.unwrap_or_else(|| "<not set>".into()));
            } else if let Some(kv) = set {
                if let Some((k, v)) = kv.split_once('=') {
                    database.set_setting(k.trim(), v.trim()).await?;
                    println!("✓ Setting '{}' updated to '{}'", k.trim(), v.trim());
                } else {
                    eprintln!("Invalid format. Use --set KEY=VALUE");
                }
            } else {
                let s = database.get_settings().await?;
                println!("=== FMD3 San (三) Current Configuration ===");
                println!("{:<24} : {}", "download_dir", s.download_dir);
                println!("{:<24} : {}", "default_format", s.default_format);
                println!("{:<24} : {}", "naming_template", s.naming_template);
                println!("{:<24} : {}", "flaresolverr_url", if s.flaresolverr_url.is_empty() { "<disabled>" } else { &s.flaresolverr_url });
                println!("{:<24} : {} hour(s)", "check_interval_hours", s.check_interval_hours);
                println!("{:<24} : {} day(s)", "catalog_resync_days", s.catalog_resync_days);
            }
        }
        Commands::Auth { action } => match action {
            AuthCommands::Login { server, username, password } => {
                let pwd = match password {
                    Some(p) => p,
                    None => {
                        eprint!("Password: ");
                        let mut line = String::new();
                        std::io::stdin().read_line(&mut line)?;
                        line.trim().to_string()
                    }
                };

                let client = reqwest::Client::new();
                let res = client
                    .post(format!("{}/api/auth/login", server.trim_end_matches('/')))
                    .json(&serde_json::json!({
                        "username": username,
                        "password": pwd
                    }))
                    .send()
                    .await?;

                if res.status().is_success() {
                    let body: serde_json::Value = res.json().await?;
                    println!("✓ Authentication successful!");
                    println!("Username : {}", body["user"]["username"]);
                    println!("Role     : {}", body["user"]["role"]);
                    if let Some(token) = body["token"].as_str() {
                        println!("Token    : {}", token);
                    }
                } else {
                    let err = res.text().await?;
                    eprintln!("✗ Authentication failed: {}", err);
                    std::process::exit(1);
                }
            }
            AuthCommands::Status { server, token } => {
                let client = reqwest::Client::new();
                let mut req = client.get(format!("{}/api/auth/status", server.trim_end_matches('/')));
                if let Some(ref t) = token {
                    req = req.header("Authorization", format!("Bearer {}", t));
                }
                let res = req.send().await?;
                if res.status().is_success() {
                    let body: serde_json::Value = res.json().await?;
                    println!("=== FMD3 San (三) Server Auth Status ===");
                    println!("Setup needed  : {}", body["setup_needed"]);
                    println!("Authenticated : {}", body["authenticated"]);
                    if let Some(u) = body.get("user").filter(|v| !v.is_null()) {
                        println!("Username      : {}", u["username"]);
                        println!("Role          : {}", u["role"]);
                        if let Some(p) = body.get("permissions").filter(|v| !v.is_null()) {
                            println!(
                                "Permissions   : download={}, favorites={}, sync={}, force_refresh={}",
                                p["can_download"], p["can_manage_favorites"], p["can_trigger_catalog_sync"], p["can_force_refresh"]
                            );
                        }
                    }
                } else {
                    eprintln!("Error querying status: {}", res.status());
                }
            }
            AuthCommands::CreateToken { username, name, expires_days, db } => {
                let database = Database::new(db).await?;
                let user_opt = database.get_user_by_username(&username).await?;
                match user_opt {
                    Some(user) => {
                        let (raw_token, token_meta) = database.create_api_token(&user.id, &name, expires_days).await?;
                        println!("✓ API Token generated successfully for user '{}'!", username);
                        println!("Token ID : {}", token_meta.id);
                        println!("Name     : {}", token_meta.name);
                        println!("Token    : {}", raw_token);
                        println!("\nUse in headers: Authorization: Bearer {}", raw_token);
                    }
                    None => {
                        eprintln!("✗ User '{}' not found in database", username);
                        std::process::exit(1);
                    }
                }
            }
            AuthCommands::ListUsers { db } => {
                let database = Database::new(db).await?;
                let users = database.list_users_with_permissions().await?;
                println!("=== Registered FMD3 Users ({}) ===", users.len());
                for (u, p) in users {
                    let active_str = if u.is_active { "Active" } else { "Inactive" };
                    println!(
                        "- {} [{}] ({}) | dl: {}, fav: {}, sync: {}, refresh: {}",
                        u.username, u.role.as_str(), active_str,
                        p.can_download, p.can_manage_favorites, p.can_trigger_catalog_sync, p.can_force_refresh
                    );
                }
            }
            AuthCommands::CreateUser { username, password, role, db } => {
                let database = Database::new(db).await?;
                let r = fmd_core::Role::from_str(&role);
                let user_count = database.count_users().await?;
                if user_count == 0 && r == fmd_core::Role::Admin {
                    let user = database.create_initial_admin(&username, &password).await?;
                    println!("✓ Initial administrator '{}' created successfully!", user.username);
                } else {
                    let user = database.create_user(&username, Some(&password), r, None, None, None).await?;
                    println!("✓ User '{}' [{}] created successfully!", user.username, user.role.as_str());
                }
            }
        },
    }

    Ok(())
}

async fn run_worker(
    nats_url: String,
    server_url: String,
    _concurrency: usize,
    modules_dir: PathBuf,
    downloads_dir: PathBuf,
    internal_token: String,
) -> anyhow::Result<()> {
    tracing::info!("Connecting to NATS at {}...", nats_url);
    let client = async_nats::connect(&nats_url).await?;
    let js = async_nats::jetstream::new(client);

    let stream = js
        .get_or_create_stream(async_nats::jetstream::stream::Config {
            name: fmd_core::STREAM_NAME.to_string(),
            subjects: vec![fmd_core::SUBJECT_TASK_ALL.to_string()],
            retention: async_nats::jetstream::stream::RetentionPolicy::WorkQueue,
            ..Default::default()
        })
        .await?;

    let consumer = stream
        .get_or_create_consumer(
            fmd_core::CONSUMER_GROUP,
            async_nats::jetstream::consumer::pull::Config {
                durable_name: Some(fmd_core::CONSUMER_GROUP.to_string()),
                ..Default::default()
            },
        )
        .await?;

    tracing::info!(
        "Worker connected to NATS stream '{}', consumer group '{}'. Server API: {}",
        fmd_core::STREAM_NAME,
        fmd_core::CONSUMER_GROUP,
        server_url
    );

    let engine = Arc::new(tokio::sync::Mutex::new(LuaModuleEngine::new()?));
    let modules = if modules_dir.exists() {
        let eng = engine.lock().await;
        eng.scan_modules_dir(&modules_dir).unwrap_or_default()
    } else {
        vec![]
    };
    let modules = Arc::new(modules);
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let mut messages = consumer.messages().await?;

    tracing::info!("Waiting for tasks from NATS JetStream...");
    while let Some(msg_res) = messages.next().await {
        match msg_res {
            Ok(msg) => {
                let task: fmd_core::TaskMessage = match serde_json::from_slice(&msg.payload) {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::error!("Failed to parse task JSON: {}", e);
                        let _ = msg.ack().await;
                        continue;
                    }
                };

                let res = process_worker_task(
                    task,
                    &http_client,
                    &server_url,
                    engine.clone(),
                    modules.clone(),
                    &modules_dir,
                    &downloads_dir,
                    &internal_token,
                )
                .await;

                if let Err(e) = res {
                    tracing::error!("Error executing worker task: {}", e);
                }

                if let Err(e) = msg.ack().await {
                    tracing::warn!("Failed to ack message to NATS: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("NATS message error: {}", e);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }

    Ok(())
}

async fn process_worker_task(
    task: fmd_core::TaskMessage,
    http_client: &reqwest::Client,
    server_url: &str,
    engine: Arc<tokio::sync::Mutex<LuaModuleEngine>>,
    modules: Arc<Vec<fmd_engine_lua::WebsiteModule>>,
    modules_dir: &Path,
    downloads_dir: &Path,
    internal_token: &str,
) -> anyhow::Result<()> {
    match task.kind {
        fmd_core::TaskKind::DownloadChapter {
            download_id,
            manga_title,
            chapter_title,
            chapter_url,
            module_id,
            format,
            retry_count: _,
            max_retries: _,
        } => {
            tracing::info!(
                "Worker processing [{}] '{}' - '{}' [module: {}, format: {}]",
                download_id, manga_title, chapter_title, module_id, format
            );

            let _ = http_client
                .post(format!("{}/api/internal/tasks/progress", server_url))
                .header("X-FMD-Internal-Token", internal_token)
                .json(&fmd_core::TaskProgressUpdate {
                    task_id: download_id.clone(),
                    status: "Downloading".into(),
                    downloaded_pages: 0,
                    total_pages: None,
                    message: None,
                })
                .send()
                .await;

            let module = modules
                .iter()
                .find(|m| m.id == module_id || m.name.eq_ignore_ascii_case(&module_id))
                .cloned();

            let (pages, maybe_wasm_module) = match module.clone() {
                Some(m) => {
                    let file_path = m.file_path.clone();
                    let eng = engine.lock().await;
                    let p = match eng.get_chapter_pages(&file_path, &chapter_url) {
                        Ok(pages_vec) if !pages_vec.is_empty() => pages_vec,
                        Ok(_) => {
                            let err = "Aucune page trouvée pour ce chapitre".to_string();
                            drop(eng);
                            let _ = http_client
                                .post(format!("{}/api/internal/tasks/complete", server_url))
                                .header("X-FMD-Internal-Token", internal_token)
                                .json(&fmd_core::TaskCompleteUpdate {
                                    task_id: download_id,
                                    success: false,
                                    error_message: Some(err),
                                    saved_path: None,
                                })
                                .send()
                                .await;
                            return Ok(());
                        }
                        Err(e) => {
                            let err = format!("Erreur extraction des pages: {}", e);
                            drop(eng);
                            let _ = http_client
                                .post(format!("{}/api/internal/tasks/complete", server_url))
                                .header("X-FMD-Internal-Token", internal_token)
                                .json(&fmd_core::TaskCompleteUpdate {
                                    task_id: download_id,
                                    success: false,
                                    error_message: Some(err),
                                    saved_path: None,
                                })
                                .send()
                                .await;
                            return Ok(());
                        }
                    };
                    (p, None)
                }
                None => {
                    let wasm_file = if module_id.ends_with(".wasm") {
                        modules_dir.join(&module_id)
                    } else {
                        modules_dir.join(format!("{}.wasm", module_id))
                    };

                    if wasm_file.exists() {
                        let wasm_engine = Arc::new(fmd_engine_wasm::WasmEngine::new()?);
                        let wasm_mod = fmd_engine_wasm::WasmMangaModule::load_from_file(wasm_engine, &wasm_file)?;
                        let p = match wasm_mod.get_chapter_pages(&chapter_url) {
                            Ok(pages_vec) if !pages_vec.is_empty() => pages_vec,
                            Ok(_) => {
                                let err = "Aucune page trouvée pour ce chapitre (WASM)".to_string();
                                let _ = http_client
                                    .post(format!("{}/api/internal/tasks/complete", server_url))
                                    .header("X-FMD-Internal-Token", internal_token)
                                    .json(&fmd_core::TaskCompleteUpdate {
                                        task_id: download_id,
                                        success: false,
                                        error_message: Some(err),
                                        saved_path: None,
                                    })
                                    .send()
                                    .await;
                                return Ok(());
                            }
                            Err(e) => {
                                let err = format!("Erreur extraction des pages WASM: {}", e);
                                let _ = http_client
                                    .post(format!("{}/api/internal/tasks/complete", server_url))
                                    .header("X-FMD-Internal-Token", internal_token)
                                    .json(&fmd_core::TaskCompleteUpdate {
                                        task_id: download_id,
                                        success: false,
                                        error_message: Some(err),
                                        saved_path: None,
                                    })
                                    .send()
                                    .await;
                                return Ok(());
                            }
                        };
                        (p, Some(wasm_mod))
                    } else {
                        let err = format!("Module '{}' introuvable dans le dossier des modules (ni Lua ni WASM)", module_id);
                        tracing::error!("{}", err);
                        let _ = http_client
                            .post(format!("{}/api/internal/tasks/complete", server_url))
                            .header("X-FMD-Internal-Token", internal_token)
                            .json(&fmd_core::TaskCompleteUpdate {
                                task_id: download_id,
                                success: false,
                                error_message: Some(err),
                                saved_path: None,
                            })
                            .send()
                            .await;
                        return Ok(());
                    }
                }
            };

            let total = pages.len();
            let _ = http_client
                .post(format!("{}/api/internal/tasks/progress", server_url))
                .header("X-FMD-Internal-Token", internal_token)
                .json(&fmd_core::TaskProgressUpdate {
                    task_id: download_id.clone(),
                    status: "Downloading".into(),
                    downloaded_pages: 0,
                    total_pages: Some(total),
                    message: None,
                })
                .send()
                .await;

            let mut packer = fmd_core::ChapterPacker::new(manga_title.clone(), chapter_title.clone());
            let net_client = fmd_net::HttpClient::new()?;
            let mut download_err = None;

            for (idx, page_url) in pages.iter().enumerate() {
                const MAX_PAGE_RETRIES: usize = 3;
                let mut page_success = false;
                let mut last_page_err = String::new();

                for attempt in 1..=MAX_PAGE_RETRIES {
                    let (headers, custom_bytes) = if let Some(ref m) = module {
                        let eng = engine.lock().await;
                        let h = eng.prepare_download_image(&m.file_path, page_url, idx).unwrap_or_default();
                        let b = eng.download_custom_image(&m.file_path, page_url, idx).ok().flatten();
                        (h, b)
                    } else {
                        (std::collections::HashMap::new(), None)
                    };

                    let page_res = match custom_bytes {
                        Some(b) => Ok(b),
                        None => net_client.get_bytes_with_headers(page_url, &headers).await,
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

                            let final_bytes = if let Some(ref wasm_mod) = maybe_wasm_module {
                                wasm_mod.process_image(&bytes, &page_name, ext).unwrap_or(bytes)
                            } else {
                                bytes
                            };

                            packer.add_page(page_name, final_bytes);

                            let _ = http_client
                                .post(format!("{}/api/internal/tasks/progress", server_url))
                                .header("X-FMD-Internal-Token", internal_token)
                                .json(&fmd_core::TaskProgressUpdate {
                                    task_id: download_id.clone(),
                                    status: "Downloading".into(),
                                    downloaded_pages: idx + 1,
                                    total_pages: Some(total),
                                    message: None,
                                })
                                .send()
                                .await;

                            page_success = true;
                            break;
                        }
                        Ok(_) => {
                            last_page_err = format!("Page {} retournée vide", idx + 1);
                        }
                        Err(e) => {
                            last_page_err = format!("Page {} échec: {}", idx + 1, e);
                        }
                    }

                    if attempt < MAX_PAGE_RETRIES {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000 * attempt as u64)).await;
                    }
                }

                if !page_success {
                    download_err = Some(last_page_err);
                    break;
                }
            }

            if let Some(err) = download_err {
                let _ = http_client
                    .post(format!("{}/api/internal/tasks/complete", server_url))
                    .header("X-FMD-Internal-Token", internal_token)
                    .json(&fmd_core::TaskCompleteUpdate {
                        task_id: download_id,
                        success: false,
                        error_message: Some(err),
                        saved_path: None,
                    })
                    .send()
                    .await;
            } else {
                let chap_num = fmd_core::extract_chapter_number(&chapter_title).unwrap_or_default();
                let (fmt, ext) = match format.to_lowercase().as_str() {
                    "cbz" | "cbr" => (fmd_core::PackagingFormat::Cbz, "cbz"),
                    "pdf" => (fmd_core::PackagingFormat::Pdf, "pdf"),
                    "epub" => (fmd_core::PackagingFormat::Epub, "epub"),
                    _ => (fmd_core::PackagingFormat::Directory, ""),
                };

                let category_str = if let Some(ref m) = module {
                    m.category.as_str()
                } else if let Some(ref wasm_mod) = maybe_wasm_module {
                    wasm_mod.meta.category.as_str()
                } else {
                    "en"
                };

                let lang_iso = match category_str.to_lowercase().as_str() {
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
                    summary: None,
                    writer: None,
                    genres: vec![],
                    web: Some(chapter_url.clone()),
                    language_iso: Some(lang_iso.to_string()),
                });

                let out_path = fmd_core::format_download_path(
                    downloads_dir,
                    "{manga}/{manga} - {chapter}",
                    &manga_title,
                    &chapter_title,
                    &chap_num,
                    &module_id,
                    ext,
                );

                match packer.pack_to(fmt, &out_path) {
                    Ok(_) => {
                        tracing::info!("Worker successfully archived [{}] to {}", download_id, out_path.display());
                        let _ = http_client
                            .post(format!("{}/api/internal/tasks/complete", server_url))
                            .header("X-FMD-Internal-Token", internal_token)
                            .json(&fmd_core::TaskCompleteUpdate {
                                task_id: download_id,
                                success: true,
                                error_message: None,
                                saved_path: Some(out_path.to_string_lossy().to_string()),
                            })
                            .send()
                            .await;
                    }
                    Err(e) => {
                        let err = format!("Erreur lors de l'archivage: {}", e);
                        tracing::error!("Worker [{}] {}", download_id, err);
                        let _ = http_client
                            .post(format!("{}/api/internal/tasks/complete", server_url))
                            .header("X-FMD-Internal-Token", internal_token)
                            .json(&fmd_core::TaskCompleteUpdate {
                                task_id: download_id,
                                success: false,
                                error_message: Some(err),
                                saved_path: None,
                            })
                            .send()
                            .await;
                    }
                }
            }
        }
        fmd_core::TaskKind::SyncCatalog { module_id } => {
            tracing::info!("Worker: Sync catalog task for module '{}' acknowledged", module_id);
        }
    }

    Ok(())
}
