use clap::{Parser, Subcommand};
use fmd_core::Database;
use fmd_engine_lua::LuaModuleEngine;
use futures_util::StreamExt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(
    name = "fmd",
    author,
    version,
    about = "Free Manga Downloader 2 - Linux Headless Daemon & Web Server"
)]
pub struct Cli {
    /// Database connection URL (sqlite://... or postgres://...)
    #[arg(long, env = "FMD_DATABASE_URL", global = true)]
    pub database_url: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
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
        /// Internal worker authentication token (required)
        #[arg(long, env = "FMD_INTERNAL_TOKEN")]
        internal_token: String,
    },
    /// Manage authentication, users, and API tokens
    Auth {
        #[command(subcommand)]
        action: AuthCommands,
    },
}

#[derive(Subcommand, Debug)]
pub enum AuthCommands {
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

pub async fn run_cli(cli: Cli) -> anyhow::Result<()> {
    let database_url = cli.database_url.clone();

    match cli.command {
        Commands::Serve { bind } => {
            fmd_server::run_server(bind, database_url).await?;
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
                                        match engine.get_chapter_pages(&m.file_path, &first_ch.url)
                                        {
                                            Ok(pages) => {
                                                println!("✓ Pages:    {} detected", pages.len());
                                                if let Some(first_page) = pages.first() {
                                                    println!("✓ Page 1:   {}", first_page);

                                                    let headers = engine
                                                        .prepare_download_image(
                                                            &m.file_path,
                                                            first_page,
                                                            0,
                                                        )
                                                        .unwrap_or_default();
                                                    println!("✓ Headers:  {:?}", headers);

                                                    let http = engine.http_client();
                                                    let custom_bytes = engine
                                                        .download_custom_image(
                                                            &m.file_path,
                                                            first_page,
                                                            0,
                                                        )
                                                        .ok()
                                                        .flatten();
                                                    let fetch_res = match custom_bytes {
                                                        Some(b) => Ok(b),
                                                        None => {
                                                            http.get_bytes_with_headers(
                                                                first_page, &headers,
                                                            )
                                                            .await
                                                        }
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
                            let wasm_mod = fmd_engine_wasm::WasmMangaModule::load_from_file(
                                wasm_engine,
                                &wasm_path,
                            )?;
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
                    println!(
                        " - [{}] {} ({}) -> {}",
                        m.category, m.name, m.root_url, m.id
                    );
                }
                if modules.len() > 20 {
                    println!(" ... and {} more modules.", modules.len() - 20);
                }
            }
        }
        Commands::ImportLegacy { path, target_db } => {
            let db_url =
                Database::resolve_url(database_url.as_deref(), &target_db.to_string_lossy());
            println!("Connecting to modern database ({})...", db_url);
            let db = Database::connect(&db_url).await?;
            println!("Importing legacy FMD2 database from {:?}...", path);
            let count = db.import_legacy_db(path).await?;
            println!("Successfully imported {} favorites/series!", count);
        }
        Commands::Catalog {
            search,
            module,
            sync,
            db,
            dir,
            limit,
        } => {
            let db_url = Database::resolve_url(database_url.as_deref(), &db.to_string_lossy());
            let database = Database::connect(&db_url).await?;

            if let Some(mod_target) = sync {
                let engine = LuaModuleEngine::new()?;
                println!("Searching for module '{}' in {:?}...", mod_target, dir);
                let modules = engine.scan_modules_dir(&dir)?;
                let m = modules
                    .into_iter()
                    .find(|m| {
                        m.name.eq_ignore_ascii_case(&mod_target)
                            || m.id.eq_ignore_ascii_case(&mod_target)
                    })
                    .ok_or_else(|| anyhow::anyhow!("Module '{}' not found", mod_target))?;

                println!(
                    "Starting catalog sync for [{}] {} (Root: {})...",
                    m.category, m.name, m.root_url
                );
                database
                    .set_module_sync_status(&m.id, "Syncing", None, None)
                    .await?;

                let cancel_token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let db_clone = database.clone();
                let m_id = m.id.clone();
                let file_path = m.file_path.clone();

                let total = tokio::task::spawn_blocking(move || {
                    let handle = tokio::runtime::Handle::current();
                    engine.update_catalog(&file_path, cancel_token, |entries, progress| {
                        let items: Vec<(String, String)> =
                            entries.into_iter().map(|e| (e.title, e.link)).collect();
                        let count = items.len();

                        handle.block_on(async {
                            let _ = db_clone.upsert_catalog_items(&m_id, &items).await;
                        });

                        let p_str = progress
                            .total_pages
                            .map(|tp| format!("{}/{}", progress.current_page + 1, tp))
                            .unwrap_or_else(|| format!("{}", progress.current_page + 1));
                        println!(
                            "  Page {} (Dir {}/{}): +{} mangas (Total: {})",
                            p_str,
                            progress.directory_index + 1,
                            progress.total_directories,
                            count,
                            progress.items_found
                        );
                        Ok(())
                    })
                })
                .await??;

                database
                    .set_module_sync_status(&m.id, "Completed", None, Some(total as i64))
                    .await?;
                println!(
                    "✓ Successfully synchronized {} mangas from module '{}'!",
                    total, m.name
                );
            } else {
                let query = search.unwrap_or_default();
                let mod_id = if let Some(ref m_name) = module {
                    let engine = LuaModuleEngine::new().ok();
                    let modules = engine
                        .and_then(|e| e.scan_modules_dir(&dir).ok())
                        .unwrap_or_default();
                    modules
                        .iter()
                        .find(|m| {
                            m.name.eq_ignore_ascii_case(m_name) || m.id.eq_ignore_ascii_case(m_name)
                        })
                        .map(|m| m.id.clone())
                        .or_else(|| Some(m_name.clone()))
                } else {
                    None
                };
                let (items, total) = database
                    .search_catalog(&query, mod_id.as_deref(), limit, 0)
                    .await?;
                println!("=== Catalog Search Results ({} total match(es)) ===", total);
                if items.is_empty() {
                    println!("No mangas found. Use `fmd catalog --sync <MODULE>` to populate local catalog.");
                } else {
                    for (idx, item) in items.iter().enumerate() {
                        println!(
                            " {:2}. [{}] {} -> {}",
                            idx + 1,
                            item.module_id,
                            item.title,
                            item.link
                        );
                    }
                }
            }
        }
        Commands::Favorites {
            list: _,
            check,
            id,
            auto_download,
            db,
            dir,
        } => {
            let database = Database::connect(&Database::resolve_url(
                database_url.as_deref(),
                &db.to_string_lossy(),
            ))
            .await?;
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

                println!(
                    "Checking {} favorite(s) for new chapters...",
                    target_favs.len()
                );
                let mut total_new = 0;

                for fav in target_favs {
                    let module = modules.iter().find(|m| {
                        m.id == fav.module_id || m.name.eq_ignore_ascii_case(&fav.module_id)
                    });
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

                            match database
                                .check_favorite_new_chapters(&fav.manga_id, &chaps)
                                .await
                            {
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
                                            println!(
                                                "    -> Queuing for download ({})",
                                                fav.download_format
                                            );
                                            for ch in new_chaps {
                                                let dl_id = uuid::Uuid::new_v4().to_string();
                                                let _ = database
                                                    .queue_download(
                                                        &dl_id,
                                                        &fav.title,
                                                        &ch.title,
                                                        &ch.url,
                                                        &fav.module_id,
                                                        &fav.download_format,
                                                        3,
                                                    )
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
                        let dl_stat =
                            format!("{}/{}", f.downloaded_chapters_count, f.total_chapters);
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
        Commands::Config {
            get,
            set,
            list: _,
            db,
        } => {
            let database = Database::connect(&Database::resolve_url(
                database_url.as_deref(),
                &db.to_string_lossy(),
            ))
            .await?;
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
                println!(
                    "{:<24} : {}",
                    "flaresolverr_url",
                    if s.flaresolverr_url.is_empty() {
                        "<disabled>"
                    } else {
                        &s.flaresolverr_url
                    }
                );
                println!(
                    "{:<24} : {} hour(s)",
                    "check_interval_hours", s.check_interval_hours
                );
                println!(
                    "{:<24} : {} day(s)",
                    "catalog_resync_days", s.catalog_resync_days
                );
            }
        }
        Commands::Auth { action } => match action {
            AuthCommands::Login {
                server,
                username,
                password,
            } => {
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
                let mut req =
                    client.get(format!("{}/api/auth/status", server.trim_end_matches('/')));
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
            AuthCommands::CreateToken {
                username,
                name,
                expires_days,
                db,
            } => {
                let database = Database::connect(&Database::resolve_url(
                    database_url.as_deref(),
                    &db.to_string_lossy(),
                ))
                .await?;
                let user_opt = database.get_user_by_username(&username).await?;
                match user_opt {
                    Some(user) => {
                        let (raw_token, token_meta) = database
                            .create_api_token(&user.id, &name, expires_days)
                            .await?;
                        println!(
                            "✓ API Token generated successfully for user '{}'!",
                            username
                        );
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
                let database = Database::connect(&Database::resolve_url(
                    database_url.as_deref(),
                    &db.to_string_lossy(),
                ))
                .await?;
                let users = database.list_users_with_permissions().await?;
                println!("=== Registered FMD3 Users ({}) ===", users.len());
                for (u, p) in users {
                    let active_str = if u.is_active { "Active" } else { "Inactive" };
                    println!(
                        "- {} [{}] ({}) | dl: {}, fav: {}, sync: {}, refresh: {}",
                        u.username,
                        u.role.as_str(),
                        active_str,
                        p.can_download,
                        p.can_manage_favorites,
                        p.can_trigger_catalog_sync,
                        p.can_force_refresh
                    );
                }
            }
            AuthCommands::CreateUser {
                username,
                password,
                role,
                db,
            } => {
                let database = Database::connect(&Database::resolve_url(
                    database_url.as_deref(),
                    &db.to_string_lossy(),
                ))
                .await?;
                let r = fmd_core::Role::from_str(&role);
                let user_count = database.count_users().await?;
                if user_count == 0 && r == fmd_core::Role::Admin {
                    let user = database.create_initial_admin(&username, &password).await?;
                    println!(
                        "✓ Initial administrator '{}' created successfully!",
                        user.username
                    );
                } else {
                    let user = database
                        .create_user(&username, Some(&password), r, None, None, None)
                        .await?;
                    println!(
                        "✓ User '{}' [{}] created successfully!",
                        user.username,
                        user.role.as_str()
                    );
                }
            }
        },
    }

    Ok(())
}

pub async fn run_worker(
    nats_url: String,
    server_url: String,
    _concurrency: usize,
    modules_dir: PathBuf,
    downloads_dir: PathBuf,
    internal_token: String,
) -> anyhow::Result<()> {
    let internal_token = internal_token.trim().to_string();
    if internal_token.is_empty() {
        anyhow::bail!("FMD_INTERNAL_TOKEN / --internal-token cannot be empty");
    }
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
        .timeout(Duration::from_secs(30))
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
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn process_worker_task(
    task: fmd_core::TaskMessage,
    http_client: &reqwest::Client,
    server_url: &str,
    engine: Arc<tokio::sync::Mutex<LuaModuleEngine>>,
    modules: Arc<Vec<fmd_engine_lua::WebsiteModule>>,
    modules_dir: &Path,
    downloads_dir: &Path,
    internal_token: &str,
) -> anyhow::Result<()> {
    process_worker_task_with_retry_delay(
        task,
        http_client,
        server_url,
        engine,
        modules,
        modules_dir,
        downloads_dir,
        internal_token,
        Duration::from_millis(1000),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn process_worker_task_with_retry_delay(
    task: fmd_core::TaskMessage,
    http_client: &reqwest::Client,
    server_url: &str,
    engine: Arc<tokio::sync::Mutex<LuaModuleEngine>>,
    modules: Arc<Vec<fmd_engine_lua::WebsiteModule>>,
    modules_dir: &Path,
    downloads_dir: &Path,
    internal_token: &str,
    retry_backoff_base: Duration,
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
                download_id,
                manga_title,
                chapter_title,
                module_id,
                format
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
                        let wasm_mod = fmd_engine_wasm::WasmMangaModule::load_from_file(
                            wasm_engine,
                            &wasm_file,
                        )?;
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
                        let err = format!(
                            "Module '{}' introuvable dans le dossier des modules (ni Lua ni WASM)",
                            module_id
                        );
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

            let mut packer =
                fmd_core::ChapterPacker::new(manga_title.clone(), chapter_title.clone());
            let net_client = fmd_net::HttpClient::new()?;
            let mut download_err = None;

            for (idx, page_url) in pages.iter().enumerate() {
                const MAX_PAGE_RETRIES: usize = 3;
                let mut page_success = false;
                let mut last_page_err = String::new();

                for attempt in 1..=MAX_PAGE_RETRIES {
                    let (headers, custom_bytes) = if let Some(ref m) = module {
                        let eng = engine.lock().await;
                        let h = eng
                            .prepare_download_image(&m.file_path, page_url, idx)
                            .unwrap_or_default();
                        let b = eng
                            .download_custom_image(&m.file_path, page_url, idx)
                            .ok()
                            .flatten();
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
                                wasm_mod
                                    .process_image(&bytes, &page_name, ext)
                                    .unwrap_or(bytes)
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

                    if attempt < MAX_PAGE_RETRIES && !retry_backoff_base.is_zero() {
                        tokio::time::sleep(retry_backoff_base * attempt as u32).await;
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
                        tracing::info!(
                            "Worker successfully archived [{}] to {}",
                            download_id,
                            out_path.display()
                        );
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
            tracing::info!(
                "Worker: Sync catalog task for module '{}' acknowledged",
                module_id
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0x63, 0x34, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn test_cli_parsing_all_subcommands_and_global_flags() {
        let cli_serve = Cli::try_parse_from([
            "fmd",
            "--database-url",
            "sqlite://x.db",
            "serve",
            "-b",
            "127.0.0.1:9090",
        ])
        .unwrap();
        assert_eq!(cli_serve.database_url.as_deref(), Some("sqlite://x.db"));
        assert!(matches!(cli_serve.command, Commands::Serve { .. }));

        let cli_worker = Cli::try_parse_from([
            "fmd",
            "worker",
            "--nats-url",
            "nats://localhost:4222",
            "--server-url",
            "http://127.0.0.1:8088",
            "--internal-token",
            "tok123",
            "--concurrency",
            "4",
        ])
        .unwrap();
        assert!(matches!(
            cli_worker.command,
            Commands::Worker { concurrency: 4, .. }
        ));

        assert!(Cli::try_parse_from(["fmd", "modules", "--check", "3asq", "--url", "u"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "import-legacy", "old.db", "-t", "new.db"]).is_ok());
        assert!(
            Cli::try_parse_from(["fmd", "catalog", "--search", "naruto", "--limit", "5"]).is_ok()
        );
        assert!(Cli::try_parse_from(["fmd", "favorites", "--check", "--auto-download"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "config", "--set", "download_dir=out"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "auth", "login", "-u", "adm", "-p", "pw"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "auth", "status", "--token", "t"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "auth", "create-token", "-u", "adm"]).is_ok());
        assert!(Cli::try_parse_from(["fmd", "auth", "list-users"]).is_ok());
        assert!(
            Cli::try_parse_from(["fmd", "auth", "create-user", "-u", "u1", "-p", "pw"]).is_ok()
        );
    }

    #[tokio::test]
    async fn test_run_worker_rejects_empty_internal_token() {
        let err = run_worker(
            "nats://127.0.0.1:4222".into(),
            "http://127.0.0.1:8088".into(),
            1,
            PathBuf::from("lua/modules"),
            PathBuf::from("downloads"),
            "   ".into(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("internal-token"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_process_worker_task_lua_fixture_cbz_success() {
        let server = MockServer::start().await;
        let base = server.uri();

        Mock::given(method("GET"))
            .and(path("/chapter/1"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"<img src="{base}/img/1.png"><img src="{base}/img/2.png">"#
            )))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/img/1.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/img/2.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/progress"))
            .and(header("X-FMD-Internal-Token", "worker-secret"))
            .respond_with(ResponseTemplate::new(200))
            .expect(4)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/complete"))
            .and(header("X-FMD-Internal-Token", "worker-secret"))
            .and(body_partial_json(serde_json::json!({
                "task_id": "dl-lua-1",
                "success": true
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let modules_dir = tempfile::tempdir().unwrap();
        let downloads_dir = tempfile::tempdir().unwrap();
        let lua_path = modules_dir.path().join("MockLua.lua");
        std::fs::write(
            &lua_path,
            format!(
                r#"
function Init()
    local m = NewWebsiteModule()
    m.ID = 'mock-lua'
    m.Name = 'MockLua'
    m.RootURL = '{base}'
    m.Category = 'French'
    m.OnGetPageNumber = 'GetPageNumber'
end

function GetPageNumber()
    if HTTP.GET(URL) then
        local doc = HTTP.Document.ToString()
        for u in string.gmatch(doc, 'src="([^"]+)"') do
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

        let engine = Arc::new(tokio::sync::Mutex::new(LuaModuleEngine::new().unwrap()));
        let modules = {
            let eng = engine.lock().await;
            Arc::new(eng.scan_modules_dir(modules_dir.path()).unwrap())
        };
        let http_client = reqwest::Client::new();
        let task = fmd_core::TaskMessage::new_download(
            "dl-lua-1".into(),
            "Solo".into(),
            "Chapitre 1".into(),
            format!("{base}/chapter/1"),
            "mock-lua".into(),
            "cbz".into(),
            0,
            3,
        );

        process_worker_task_with_retry_delay(
            task,
            &http_client,
            &base,
            engine,
            modules,
            modules_dir.path(),
            downloads_dir.path(),
            "worker-secret",
            Duration::from_millis(5),
        )
        .await
        .unwrap();

        let expected_cbz = downloads_dir.path().join("Solo/Solo - Chapitre 1.cbz");
        assert!(expected_cbz.exists(), "CBZ output should exist");
        let zip = zip::ZipArchive::new(std::fs::File::open(expected_cbz).unwrap()).unwrap();
        assert_eq!(zip.len(), 3); // 001.png, 002.png, ComicInfo.xml
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_process_worker_task_wasm_fixture_success() {
        let server = MockServer::start().await;
        let base = server.uri();

        Mock::given(method("GET"))
            .and(path("/wasm/1.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/progress"))
            .and(header("X-FMD-Internal-Token", "worker-secret"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/complete"))
            .and(header("X-FMD-Internal-Token", "worker-secret"))
            .and(body_partial_json(serde_json::json!({
                "task_id": "dl-wasm-1",
                "success": true
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let meta_json = r#"{"id":"mock-wasm","name":"MockWasm","version":"1.0.0","author":"Test","root_url":"https://mock.wasm","category":"English"}"#;
        let pages_json = format!(r#"{{"pages":["{base}/wasm/1.png"]}}"#);
        let escape_wat = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let wat = format!(
            r#"
            (module
              (import "env" "host_log" (func $host_log (param i32 i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 1024) "{meta}")
              (data (i32.const 4096) "{pages}")
              (func (export "_fmd_meta") (result i64)
                (i64.or (i64.shl (i64.const 1024) (i64.const 32)) (i64.const {meta_len}))
              )
              (func (export "_fmd_get_pages") (param i32 i32) (result i64)
                (i64.or (i64.shl (i64.const 4096) (i64.const 32)) (i64.const {pages_len}))
              )
              (func (export "_fmd_process_image") (param $ptr i32) (param $len i32) (result i64)
                (i64.or (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32)) (i64.extend_i32_u (local.get $len)))
              )
            )
            "#,
            meta = escape_wat(meta_json),
            meta_len = meta_json.len(),
            pages = escape_wat(&pages_json),
            pages_len = pages_json.len()
        );
        let wasm_bytes = wat::parse_str(wat).unwrap();

        let modules_dir = tempfile::tempdir().unwrap();
        let downloads_dir = tempfile::tempdir().unwrap();
        std::fs::write(modules_dir.path().join("mock-wasm.wasm"), wasm_bytes).unwrap();

        let engine = Arc::new(tokio::sync::Mutex::new(LuaModuleEngine::new().unwrap()));
        let http_client = reqwest::Client::new();
        let task = fmd_core::TaskMessage::new_download(
            "dl-wasm-1".into(),
            "WasmTitle".into(),
            "Ch 1".into(),
            "https://mock.wasm/ch/1".into(),
            "mock-wasm".into(),
            "cbz".into(),
            0,
            3,
        );

        process_worker_task_with_retry_delay(
            task,
            &http_client,
            &base,
            engine,
            Arc::new(vec![]),
            modules_dir.path(),
            downloads_dir.path(),
            "worker-secret",
            Duration::from_millis(5),
        )
        .await
        .unwrap();

        assert!(downloads_dir
            .path()
            .join("WasmTitle/WasmTitle - Ch 1.cbz")
            .exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_process_worker_task_missing_module_page_failures_and_sync() {
        let server = MockServer::start().await;
        let base = server.uri();

        // Missing module failure callback
        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/progress"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/complete"))
            .and(body_partial_json(serde_json::json!({
                "task_id": "dl-missing",
                "success": false
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;

        let modules_dir = tempfile::tempdir().unwrap();
        let downloads_dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(tokio::sync::Mutex::new(LuaModuleEngine::new().unwrap()));
        let http_client = reqwest::Client::new();

        process_worker_task_with_retry_delay(
            fmd_core::TaskMessage::new_download(
                "dl-missing".into(),
                "Manga".into(),
                "Ch 1".into(),
                "https://example.com/1".into(),
                "does-not-exist".into(),
                "cbz".into(),
                0,
                3,
            ),
            &http_client,
            &base,
            engine.clone(),
            Arc::new(vec![]),
            modules_dir.path(),
            downloads_dir.path(),
            "worker-secret",
            Duration::from_millis(5),
        )
        .await
        .unwrap();

        // Page 500 failure after 3 retries
        let fail_server = MockServer::start().await;
        let fail_base = fail_server.uri();
        Mock::given(method("GET"))
            .and(path("/ch/fail"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!(r#"<img src="{fail_base}/bad.png">"#)),
            )
            .mount(&fail_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/bad.png"))
            .respond_with(ResponseTemplate::new(500))
            .expect(3)
            .mount(&fail_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/progress"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&fail_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/internal/tasks/complete"))
            .and(body_partial_json(serde_json::json!({
                "task_id": "dl-fail-page",
                "success": false
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&fail_server)
            .await;

        std::fs::write(
            modules_dir.path().join("FailLua.lua"),
            format!(
                r#"
function Init()
    local m = NewWebsiteModule()
    m.ID = 'fail-lua'
    m.Name = 'FailLua'
    m.RootURL = '{fail_base}'
    m.OnGetPageNumber = 'GetPageNumber'
end
function GetPageNumber()
    if HTTP.GET(URL) then
        for u in string.gmatch(HTTP.Document.ToString(), 'src="([^"]+)"') do
            TASK.PageLinks.Add(u)
        end
    end
end
"#
            ),
        )
        .unwrap();
        let mods = {
            let eng = engine.lock().await;
            Arc::new(eng.scan_modules_dir(modules_dir.path()).unwrap())
        };

        process_worker_task_with_retry_delay(
            fmd_core::TaskMessage::new_download(
                "dl-fail-page".into(),
                "Manga".into(),
                "Ch 1".into(),
                format!("{fail_base}/ch/fail"),
                "fail-lua".into(),
                "cbz".into(),
                0,
                3,
            ),
            &http_client,
            &fail_base,
            engine.clone(),
            mods.clone(),
            modules_dir.path(),
            downloads_dir.path(),
            "worker-secret",
            Duration::from_millis(5),
        )
        .await
        .unwrap();

        // SyncCatalog task returns Ok(()) cleanly
        let sync_task = fmd_core::TaskMessage::new_sync("fail-lua".into());
        process_worker_task_with_retry_delay(
            sync_task,
            &http_client,
            &fail_base,
            engine,
            mods,
            modules_dir.path(),
            downloads_dir.path(),
            "worker-secret",
            Duration::from_millis(5),
        )
        .await
        .unwrap();
    }
}
