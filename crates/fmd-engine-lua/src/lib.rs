pub mod bindings;
pub mod mangainfo;
pub mod module;

pub use mangainfo::{ChapterItem, MangaInfoResult};
pub use module::WebsiteModule;

use bindings::{
    crypto::register_crypto,
    duktape::register_duktape,
    env::register_env,
    http::register_http,
    mangafox::register_mangafox,
    pb::register_pb,
    strings::{create_alternate_versions_table, create_string_list_table},
    xquery::register_xquery,
};
use fmd_net::HttpClient;
use mlua::{HookTriggers, Lua, Table, VmState};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tracing::{info, warn};

pub mod bindings_duktape {
    pub use super::bindings::duktape::*;
}

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Lua error: {0}")]
    Lua(#[from] mlua::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Engine error: {0}")]
    Custom(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CatalogEntry {
    pub title: String,
    pub link: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CatalogSyncProgress {
    pub module_id: String,
    pub directory_index: usize,
    pub total_directories: usize,
    pub current_page: usize,
    pub total_pages: Option<usize>,
    pub items_found: usize,
}

const LUA_MEMORY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
const LUA_MAX_INSTRUCTIONS_PER_CALL: u64 = 50_000_000;
const LUA_CALL_TIMEOUT_SECS: u64 = 30;
const LUA_HOOK_STEP_INTERVAL: u32 = 10_000;

struct ExecutionBudget {
    deadline: Instant,
    steps: u64,
    max_steps: u64,
}

impl ExecutionBudget {
    fn new() -> Self {
        Self {
            deadline: Instant::now() + Duration::from_secs(LUA_CALL_TIMEOUT_SECS),
            steps: 0,
            max_steps: LUA_MAX_INSTRUCTIONS_PER_CALL,
        }
    }

    fn reset(&mut self) {
        self.deadline = Instant::now() + Duration::from_secs(LUA_CALL_TIMEOUT_SECS);
        self.steps = 0;
        self.max_steps = LUA_MAX_INSTRUCTIONS_PER_CALL;
    }
}

fn normalize_lexical(path: &Path) -> std::result::Result<PathBuf, String> {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err("Path traversal escapes root".into());
                }
            }
            Component::Normal(seg) => out.push(seg),
        }
    }
    Ok(out)
}

fn canonicalize_with_tail(path: &Path) -> std::result::Result<PathBuf, String> {
    let normalized = normalize_lexical(path)?;
    let mut existing = normalized.as_path();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();

    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| "Unable to resolve path ancestor".to_string())?;
        tail.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| "Unable to resolve path parent".to_string())?;
    }

    let mut resolved = std::fs::canonicalize(existing)
        .map_err(|e| format!("Failed to resolve path '{}': {}", path.display(), e))?;
    for seg in tail.into_iter().rev() {
        resolved.push(seg);
    }
    Ok(resolved)
}

fn sandbox_allowed_roots(write: bool) -> std::result::Result<Vec<PathBuf>, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to resolve current working directory: {}", e))?;
    let userdata_dir = cwd.join("userdata");
    if write {
        std::fs::create_dir_all(&userdata_dir)
            .map_err(|e| format!("Failed to prepare Lua userdata directory: {}", e))?;
        let canonical_userdata = std::fs::canonicalize(&userdata_dir)
            .map_err(|e| format!("Failed to canonicalize Lua userdata directory: {}", e))?;
        return Ok(vec![canonical_userdata]);
    }

    let mut roots = Vec::new();
    for candidate in [cwd.join("lua"), cwd.join("../../lua"), userdata_dir] {
        if candidate.exists() {
            if let Ok(canon) = std::fs::canonicalize(&candidate) {
                roots.push(canon);
            }
        }
    }
    if roots.is_empty() {
        return Err("No readable Lua sandbox root exists".into());
    }
    Ok(roots)
}

fn resolve_sandboxed_path(raw_path: &str, write: bool) -> std::result::Result<PathBuf, String> {
    if raw_path.is_empty() || raw_path.contains('\0') {
        return Err("Invalid empty or NUL-containing path".into());
    }
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Failed to resolve current working directory: {}", e))?;
    let candidate = if Path::new(raw_path).is_absolute() {
        PathBuf::from(raw_path)
    } else if !write
        && !cwd.join("lua").exists()
        && cwd.join("../../lua").exists()
        && (raw_path.starts_with("lua/") || raw_path.starts_with("lua\\"))
    {
        cwd.join("../..").join(raw_path)
    } else {
        cwd.join(raw_path)
    };
    let resolved = canonicalize_with_tail(&candidate)?;
    let roots = sandbox_allowed_roots(write)?;
    if roots.iter().any(|root| resolved.starts_with(root)) {
        Ok(resolved)
    } else {
        Err(format!(
            "Access to path '{}' is forbidden by Lua security sandbox",
            raw_path
        ))
    }
}

pub struct LuaModuleEngine {
    lua: Lua,
    modules: Arc<Mutex<Vec<WebsiteModule>>>,
    http_client: HttpClient,
    links_items: Arc<Mutex<Vec<String>>>,
    names_items: Arc<Mutex<Vec<String>>>,
    execution_budget: Arc<Mutex<ExecutionBudget>>,
}

impl LuaModuleEngine {
    pub fn new() -> Result<Self> {
        Self::with_http_client(HttpClient::new().map_err(|e| EngineError::Custom(e.to_string()))?)
    }

    pub fn new_public_only() -> Result<Self> {
        Self::with_http_client(
            HttpClient::new_public_only().map_err(|e| EngineError::Custom(e.to_string()))?,
        )
    }

    fn reset_execution_budget(&self) {
        if let Ok(mut budget) = self.execution_budget.lock() {
            budget.reset();
        }
    }

    fn with_http_client(http_client: HttpClient) -> Result<Self> {
        let lua = Lua::new();
        lua.set_memory_limit(LUA_MEMORY_LIMIT_BYTES)?;

        let execution_budget = Arc::new(Mutex::new(ExecutionBudget::new()));
        let hook_budget = execution_budget.clone();
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(LUA_HOOK_STEP_INTERVAL),
            move |_lua, _debug| {
                let mut guard = hook_budget.lock().map_err(|_| {
                    mlua::Error::RuntimeError("Lua execution budget poisoned".into())
                })?;
                guard.steps = guard.steps.saturating_add(LUA_HOOK_STEP_INTERVAL as u64);
                if guard.steps > guard.max_steps || Instant::now() > guard.deadline {
                    return Err(mlua::Error::RuntimeError(
                        "Lua execution limit exceeded".into(),
                    ));
                }
                Ok(VmState::Continue)
            },
        );

        let modules = Arc::new(Mutex::new(Vec::new()));

        // 1. Configure package.path and sandbox dangerous package capabilities
        let package: Table = lua.globals().get("package")?;
        let _ = package.set("loadlib", mlua::Value::Nil);
        let _ = package.set("cpath", "");
        if let Ok(searchers) = package.get::<Table>("searchers") {
            // Keep only preload (1) and Lua path loader (2); strip C library searchers.
            let len = searchers.raw_len();
            for idx in 3..=len {
                let _ = searchers.raw_set(idx, mlua::Value::Nil);
            }
        }

        let current_path: String = package.get("path")?;
        let lua_paths = "lua/?.lua;lua/?/init.lua;lua/templates/?.lua;lua/utils/?.lua;../../lua/?.lua;../../lua/?/init.lua;../../lua/templates/?.lua;../../lua/utils/?.lua;?.lua";
        let new_path = format!("{};{}", lua_paths, current_path);
        package.set("path", new_path)?;

        // 2. Harden Lua Standard Environment (Defense-in-Depth Sandbox)
        let globals = lua.globals();
        let _ = globals.set("load", mlua::Value::Nil);
        let _ = globals.set("loadfile", mlua::Value::Nil);
        let _ = globals.set("dofile", mlua::Value::Nil);
        let _ = globals.set("loadstring", mlua::Value::Nil);

        if let Ok(os_tbl) = globals.get::<Table>("os") {
            let safe_os = lua.create_table()?;
            for key in ["time", "date", "difftime", "clock"] {
                if let Ok(val) = os_tbl.get::<mlua::Value>(key) {
                    let _ = safe_os.set(key, val);
                }
            }
            let _ = globals.set("os", safe_os);
        }

        if let Ok(io_tbl) = globals.get::<Table>("io") {
            let _ = io_tbl.set("popen", mlua::Value::Nil);
            let _ = io_tbl.set("tmpfile", mlua::Value::Nil);

            if let Ok(orig_open) = io_tbl.get::<mlua::Function>("open") {
                let safe_open =
                    lua.create_function(move |_lua, (path, mode): (String, Option<String>)| {
                        let mode_str = mode.as_deref().unwrap_or("r");
                        let write = mode_str.contains('w')
                            || mode_str.contains('a')
                            || mode_str.contains('+');
                        let resolved = resolve_sandboxed_path(&path, write)
                            .map_err(mlua::Error::RuntimeError)?;
                        orig_open
                            .call::<mlua::Value>((resolved.to_string_lossy().into_owned(), mode))
                    })?;
                let _ = io_tbl.set("open", safe_open);
            }

            if let Ok(orig_lines) = io_tbl.get::<mlua::Function>("lines") {
                let safe_lines = lua.create_function(move |lua, args: mlua::MultiValue| {
                    let mut out_args = args.into_vec();
                    if let Some(mlua::Value::String(path)) = out_args.first() {
                        let raw = path.to_str()?.to_string();
                        let resolved = resolve_sandboxed_path(&raw, false)
                            .map_err(mlua::Error::RuntimeError)?;
                        let s = lua.create_string(resolved.to_string_lossy().as_bytes())?;
                        out_args[0] = mlua::Value::String(s);
                    }
                    orig_lines.call::<mlua::Value>(mlua::MultiValue::from_vec(out_args))
                })?;
                let _ = io_tbl.set("lines", safe_lines);
            }

            if let Ok(orig_input) = io_tbl.get::<mlua::Function>("input") {
                let safe_input =
                    lua.create_function(move |lua, arg: Option<mlua::Value>| match arg {
                        Some(mlua::Value::String(path)) => {
                            let raw = path.to_str()?.to_string();
                            let resolved = resolve_sandboxed_path(&raw, false)
                                .map_err(mlua::Error::RuntimeError)?;
                            let s = lua.create_string(resolved.to_string_lossy().as_bytes())?;
                            orig_input.call::<mlua::Value>(Some(mlua::Value::String(s)))
                        }
                        other => orig_input.call::<mlua::Value>(other),
                    })?;
                let _ = io_tbl.set("input", safe_input);
            }

            if let Ok(orig_output) = io_tbl.get::<mlua::Function>("output") {
                let safe_output =
                    lua.create_function(move |lua, arg: Option<mlua::Value>| match arg {
                        Some(mlua::Value::String(path)) => {
                            let raw = path.to_str()?.to_string();
                            let resolved = resolve_sandboxed_path(&raw, true)
                                .map_err(mlua::Error::RuntimeError)?;
                            let s = lua.create_string(resolved.to_string_lossy().as_bytes())?;
                            orig_output.call::<mlua::Value>(Some(mlua::Value::String(s)))
                        }
                        other => orig_output.call::<mlua::Value>(other),
                    })?;
                let _ = io_tbl.set("output", safe_output);
            }
        }

        // 3. Set global status constants
        globals.set("no_error", 0)?;
        globals.set("net_problem", 1)?;
        globals.set("information_not_found", 2)?;

        let maybe_fill_host = lua.create_function(|_, (host, url): (String, String)| {
            let u = url.trim();
            if u.starts_with("http://") || u.starts_with("https://") {
                Ok(u.to_string())
            } else {
                let base = host.trim_end_matches('/');
                let path = if u.starts_with('/') {
                    u.to_string()
                } else {
                    format!("/{}", u)
                };
                Ok(format!("{}{}", base, path))
            }
        })?;
        globals.set("MaybeFillHost", maybe_fill_host)?;

        let fill_host = lua.create_function(|_, (host, url): (String, String)| {
            let u = url.trim();
            let base = host.trim_end_matches('/');
            let path = if let Ok(parsed) = url::Url::parse(u) {
                parsed.path().to_string()
            } else if u.starts_with('/') {
                u.to_string()
            } else {
                format!("/{}", u)
            };
            Ok(format!("{}{}", base, path))
        })?;
        globals.set("FillHost", fill_host)?;

        let trim_fn = lua.create_function(|_, (s, chars): (String, Option<String>)| {
            if let Some(c) = chars {
                Ok(s.trim_matches(|ch| c.contains(ch)).to_string())
            } else {
                Ok(s.trim().to_string())
            }
        })?;
        globals.set("Trim", trim_fn)?;

        type MangaStatusParams = (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        );

        let mangainfo_status_if_pos =
            lua.create_function(|_, (search, o, c, h, ca): MangaStatusParams| {
                let s = search.unwrap_or_default().to_lowercase();
                let o = o.unwrap_or_else(|| "ongoing".into()).to_lowercase();
                let c = c.unwrap_or_else(|| "completed".into()).to_lowercase();
                let h = h.unwrap_or_else(|| "hiatus".into()).to_lowercase();
                let ca = ca.unwrap_or_else(|| "cancelled".into()).to_lowercase();

                let check = |pattern: &str| -> bool {
                    for part in pattern.split('|') {
                        if !part.is_empty() && s.contains(part) {
                            return true;
                        }
                    }
                    false
                };

                if check(&o) {
                    Ok("ongoing".to_string())
                } else if check(&c) {
                    Ok("completed".to_string())
                } else if check(&h) {
                    Ok("hiatus".to_string())
                } else if check(&ca) {
                    Ok("cancelled".to_string())
                } else {
                    Ok("unknown".to_string())
                }
            })?;
        globals.set("MangaInfoStatusIfPos", mangainfo_status_if_pos)?;

        // String storage containers
        let (links_tbl, links_items) = create_string_list_table(&lua)?;
        let (names_tbl, names_items) = create_string_list_table(&lua)?;
        globals.set("LINKS", links_tbl)?;
        globals.set("NAMES", names_tbl)?;
        globals.set("TASK", lua.create_table()?)?;
        globals.set("MANGAINFO", lua.create_table()?)?;

        let updatelist_tbl = lua.create_table()?;
        updatelist_tbl.set("CurrentDirectoryPageNumber", 0)?;
        updatelist_tbl.set(
            "UpdateStatusText",
            lua.create_function(|_, _: String| Ok(()))?,
        )?;
        globals.set("UPDATELIST", updatelist_tbl)?;
        globals.set("PAGENUMBER", 0)?;

        // Global debug table for modules using debug.getinfo
        let debug_table = lua.create_table()?;
        let getinfo =
            lua.create_function(|lua, (_level, _what): (Option<i32>, Option<String>)| {
                let t = lua.create_table()?;
                t.set("source", "@lua/modules/dummy.lua")?;
                t.set("short_src", "dummy.lua")?;
                Ok(t)
            })?;
        debug_table.set("getinfo", getinfo)?;
        globals.set("debug", debug_table)?;

        // 3. Register bindings
        register_http(&lua, http_client.clone())?;
        register_xquery(&lua)?;
        register_duktape(&lua)?;
        register_env(&lua)?;
        register_crypto(&lua)?;
        register_mangafox(&lua)?;
        register_pb(&lua)?;

        // 4. Register NewWebsiteModule()
        let new_website_module = lua.create_function_mut(move |lua, ()| {
            let m_table = lua.create_table()?;

            // Helper methods on module
            let storage = lua.create_table()?;
            let storage_meta = lua.create_table()?;
            let index_fn = lua.create_function(|_, (_, _): (mlua::Table, mlua::Value)| Ok(""))?;
            storage_meta.set("__index", index_fn)?;
            storage.set_metatable(Some(storage_meta));
            m_table.set("Storage", storage)?;

            let options_table = lua.create_table()?;
            m_table.set("Options", options_table.clone())?;

            let opt_cb = options_table.clone();
            m_table.set(
                "AddOptionComboBox",
                lua.create_function(
                    move |_,
                          (name, _caption, _items, default_idx): (
                        String,
                        String,
                        Option<String>,
                        Option<i32>,
                    )| {
                        let _ = opt_cb.set(name, default_idx.unwrap_or(0));
                        Ok(())
                    },
                )?,
            )?;

            let opt_check = options_table.clone();
            m_table.set(
                "AddOptionCheckBox",
                lua.create_function(
                    move |_, (name, _caption, default_val): (String, String, Option<bool>)| {
                        let _ = opt_check.set(name, default_val.unwrap_or(false));
                        Ok(())
                    },
                )?,
            )?;

            let opt_spin = options_table.clone();
            m_table.set(
                "AddOptionSpinEdit",
                lua.create_function(
                    move |_, (name, _caption, default_val): (String, String, Option<i32>)| {
                        let _ = opt_spin.set(name, default_val.unwrap_or(0));
                        Ok(())
                    },
                )?,
            )?;

            let opt_edit = options_table.clone();
            m_table.set(
                "AddOptionEdit",
                lua.create_function(
                    move |_, (name, _caption, default_val): (String, String, Option<String>)| {
                        let _ = opt_edit.set(name, default_val.unwrap_or_default());
                        Ok(())
                    },
                )?,
            )?;

            let opt_get = options_table.clone();
            m_table.set(
                "GetOption",
                lua.create_function(move |_, name: String| {
                    let val: Option<mlua::Value> = opt_get.get(name).ok();
                    Ok(val)
                })?,
            )?;

            m_table.set("TotalDirectory", 1)?;
            m_table.set("CurrentDirectoryIndex", 0)?;

            // Global MODULE reference
            lua.globals().set("MODULE", m_table.clone())?;

            Ok(m_table)
        })?;

        globals.set("NewWebsiteModule", new_website_module)?;

        Ok(Self {
            lua,
            modules,
            http_client,
            links_items,
            names_items,
            execution_budget,
        })
    }

    pub fn load_file<P: AsRef<Path>>(&self, path: P) -> Result<Option<WebsiteModule>> {
        self.reset_execution_budget();
        let path_ref = path.as_ref();
        let mut content = std::fs::read_to_string(path_ref)?;
        // Strip UTF-8 BOM if present
        if content.starts_with('\u{feff}') {
            content = content[3..].to_string();
        }

        // Execute file to define functions like Init, GetInfo, etc.
        self.lua
            .load(&content)
            .set_name(format!("@{}", path_ref.display()))
            .exec()?;

        // Call Init()
        if let Ok(init_fn) = self.lua.globals().get::<mlua::Function>("Init") {
            let _: () = init_fn.call(())?;

            // Read the registered MODULE table
            if let Ok(m_table) = self.lua.globals().get::<Table>("MODULE") {
                let id: String = m_table.get("ID").unwrap_or_default();
                let name: String = m_table.get("Name").unwrap_or_default();
                let root_url: String = m_table.get("RootURL").unwrap_or_default();
                let category: String = m_table.get("Category").unwrap_or_default();
                let on_get_directory_page_number: Option<String> =
                    m_table.get("OnGetDirectoryPageNumber").ok();
                let total_directory: usize = m_table.get("TotalDirectory").unwrap_or(1);

                let module = WebsiteModule {
                    id,
                    name,
                    root_url,
                    category,
                    file_path: path_ref.to_string_lossy().to_string(),
                    on_get_name_and_link: m_table.get("OnGetNameAndLink").ok(),
                    on_get_info: m_table.get("OnGetInfo").ok(),
                    on_get_page_number: m_table.get("OnGetPageNumber").ok(),
                    on_get_image_url: m_table.get("OnGetImageURL").ok(),
                    on_before_download_image: m_table.get("OnBeforeDownloadImage").ok(),
                    on_download_image: m_table.get("OnDownloadImage").ok(),
                    on_get_directory_page_number,
                    total_directory,
                };

                return Ok(Some(module));
            }
        }

        Ok(None)
    }

    pub fn scan_modules_dir<P: AsRef<Path>>(&self, dir: P) -> Result<Vec<WebsiteModule>> {
        let mut loaded = Vec::new();
        let entries = std::fs::read_dir(dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("lua") {
                match self.load_file(&path) {
                    Ok(Some(m)) => {
                        loaded.push(m);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        warn!("Skipping module {:?}: {}", path, e);
                    }
                }
            }
        }

        info!("Discovered {} valid FMD3 Lua website modules", loaded.len());
        Ok(loaded)
    }

    pub fn get_manga_info(&self, file_path: &str, manga_url: &str) -> Result<MangaInfoResult> {
        // Load the module script
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let on_get_info = module.on_get_info.unwrap_or_else(|| "GetInfo".into());

        // Prepare globals
        let globals = self.lua.globals();
        globals.set("URL", manga_url)?;

        let mangainfo_table = self.lua.create_table()?;
        mangainfo_table.set("URL", manga_url)?;
        let (ch_names_tbl, ch_names_items) = create_string_list_table(&self.lua)?;
        let (ch_links_tbl, ch_links_items) = create_string_list_table(&self.lua)?;
        let (alt_tbl, alt_items) = create_alternate_versions_table(&self.lua)?;

        mangainfo_table.set("ChapterNames", ch_names_tbl)?;
        mangainfo_table.set("ChapterLinks", ch_links_tbl)?;
        mangainfo_table.set("AlternateVersions", alt_tbl)?;
        globals.set("MANGAINFO", mangainfo_table)?;

        // Call OnGetInfo
        self.reset_execution_budget();
        if let Ok(info_fn) = globals.get::<mlua::Function>(&*on_get_info) {
            let _: mlua::Value = info_fn.call(())?;
        }

        // Read results back from MANGAINFO
        let mangainfo: Table = globals.get("MANGAINFO")?;
        let title: String = mangainfo
            .get("Title")
            .unwrap_or_else(|_| "Unknown Title".into());
        let cover_url: Option<String> = mangainfo.get("CoverLink").ok();
        let authors: Option<String> = mangainfo.get("Authors").ok();
        let artists: Option<String> = mangainfo.get("Artists").ok();
        let status: String = mangainfo.get("Status").unwrap_or_else(|_| "ongoing".into());
        let summary: Option<String> = mangainfo.get("Summary").ok();

        let raw_genres: String = mangainfo.get("Genres").unwrap_or_default();
        let genres = raw_genres
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Extract chapters
        let mut chapters = Vec::new();
        let names = ch_names_items.lock().map(|i| i.clone()).unwrap_or_default();
        let links = ch_links_items.lock().map(|i| i.clone()).unwrap_or_default();

        for (idx, name) in names.into_iter().enumerate() {
            let url = links.get(idx).cloned().unwrap_or_default();
            chapters.push(ChapterItem { title: name, url });
        }

        let alternate_versions = alt_items.lock().map(|i| i.clone()).unwrap_or_default();

        Ok(MangaInfoResult {
            title,
            original_url: manga_url.to_string(),
            cover_url,
            authors,
            artists,
            genres,
            status,
            summary,
            chapters,
            alternate_versions,
        })
    }

    pub fn get_chapter_pages(&self, file_path: &str, chapter_url: &str) -> Result<Vec<String>> {
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let on_get_page_number = module
            .on_get_page_number
            .unwrap_or_else(|| "GetPageNumber".into());
        let on_get_image_url = module
            .on_get_image_url
            .unwrap_or_else(|| "GetImageURL".into());

        let globals = self.lua.globals();
        globals.set("URL", chapter_url)?;

        let task_table = self.lua.create_table()?;
        let (page_links_tbl, page_links_items) = create_string_list_table(&self.lua)?;
        let (page_containers_tbl, page_containers_items) = create_string_list_table(&self.lua)?;

        task_table.set("PageLinks", page_links_tbl)?;
        task_table.set("PageContainerLinks", page_containers_tbl)?;
        task_table.set("PageNumber", 0)?;
        globals.set("TASK", task_table)?;

        // Execute OnGetPageNumber
        self.reset_execution_budget();
        if let Ok(page_fn) = globals.get::<mlua::Function>(&*on_get_page_number) {
            let _: mlua::Value = page_fn.call(())?;
        }

        // If PageLinks already filled, deduplicate and return them
        let raw_links = page_links_items
            .lock()
            .map(|i| i.clone())
            .unwrap_or_default();
        let mut seen_links = std::collections::HashSet::new();
        let mut final_urls: Vec<String> = Vec::new();
        for u in raw_links {
            if !u.is_empty() && seen_links.insert(u.clone()) {
                final_urls.push(u);
            }
        }
        if !final_urls.is_empty() {
            return Ok(final_urls);
        }

        // Otherwise, resolve PageContainerLinks via OnGetImageURL
        let raw_containers = page_containers_items
            .lock()
            .map(|i| i.clone())
            .unwrap_or_default();
        let mut seen_c = std::collections::HashSet::new();
        let mut containers: Vec<String> = Vec::new();
        for c in raw_containers {
            if !c.is_empty() && seen_c.insert(c.clone()) {
                containers.push(c);
            }
        }

        // Sync deduplicated containers back to PageContainerLinks
        if let Ok(mut items) = page_containers_items.lock() {
            *items = containers.clone();
        }

        let task: Table = globals.get("TASK")?;
        let page_num: i64 = task.get("PageNumber").unwrap_or(0);
        let total_count = if !containers.is_empty() {
            containers.len()
        } else {
            page_num as usize
        };

        if total_count > 0 {
            if let Ok(img_fn) = globals.get::<mlua::Function>(&*on_get_image_url) {
                for i in 0..total_count {
                    self.reset_execution_budget();
                    globals.set("WORKID", i as i64)?;
                    let _: mlua::Value = img_fn.call(())?;
                }
            }
            let raw_resolved = page_links_items
                .lock()
                .map(|i| i.clone())
                .unwrap_or_default();
            let mut seen_r = std::collections::HashSet::new();
            final_urls.clear();
            for u in raw_resolved {
                if !u.is_empty() && seen_r.insert(u.clone()) {
                    final_urls.push(u);
                }
            }
        }

        Ok(final_urls)
    }

    pub fn prepare_download_image(
        &self,
        file_path: &str,
        image_url: &str,
        work_id: usize,
    ) -> Result<std::collections::HashMap<String, String>> {
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let globals = self.lua.globals();
        globals.set("URL", image_url)?;
        globals.set("WORKID", work_id as i64)?;
        globals.set("WorkId", work_id as i64)?;

        if let Some(on_before) = module.on_before_download_image {
            if let Ok(hook_fn) = globals.get::<mlua::Function>(&*on_before) {
                self.reset_execution_budget();
                let _: mlua::Value = hook_fn.call(())?;
            }
        }

        let mut headers = std::collections::HashMap::new();
        if let Ok(http) = globals.get::<Table>("HTTP") {
            if let Ok(headers_tbl) = http.get::<Table>("Headers") {
                if let Ok(values_tbl) = headers_tbl.get::<Table>("Values") {
                    for (k, v) in values_tbl.pairs::<String, String>().flatten() {
                        headers.insert(k, v);
                    }
                }
            }
        }

        Ok(headers)
    }

    pub fn download_custom_image(
        &self,
        file_path: &str,
        image_url: &str,
        work_id: usize,
    ) -> Result<Option<Vec<u8>>> {
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let on_download = match module.on_download_image {
            Some(f) => f,
            None => return Ok(None),
        };

        let globals = self.lua.globals();
        globals.set("URL", image_url)?;
        globals.set("WORKID", work_id as i64)?;
        globals.set("WorkId", work_id as i64)?;

        if let Ok(hook_fn) = globals.get::<mlua::Function>(&*on_download) {
            self.reset_execution_budget();
            let res: mlua::Value = hook_fn.call(())?;
            match res {
                mlua::Value::Boolean(true) => {
                    if let Ok(http) = globals.get::<Table>("HTTP") {
                        if let Ok(doc) = http.get::<Table>("Document") {
                            if let Ok(to_string) = doc.get::<mlua::Function>("ToString") {
                                if let Ok(s) = to_string.call::<mlua::String>(()) {
                                    return Ok(Some(s.as_bytes().to_vec()));
                                }
                            }
                        }
                    }
                    Ok(None)
                }
                mlua::Value::String(s) => Ok(Some(s.as_bytes().to_vec())),
                mlua::Value::Boolean(false) => Err(EngineError::Custom(format!(
                    "OnDownloadImage failed for {}",
                    image_url
                ))),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    pub fn fetch_catalog_page(
        &self,
        file_path: &str,
        dir_index: usize,
        page: usize,
    ) -> Result<(Vec<CatalogEntry>, Option<usize>)> {
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let on_get_name_and_link = module.on_get_name_and_link.ok_or_else(|| {
            EngineError::Custom(
                "Module does not support catalog extraction (OnGetNameAndLink is nil)".into(),
            )
        })?;

        let globals = self.lua.globals();

        if let Ok(m_tbl) = globals.get::<Table>("MODULE") {
            let _ = m_tbl.set("CurrentDirectoryIndex", dir_index as i64);
        }
        globals.set("URL", page as i64)?;

        if let Ok(mut it) = self.links_items.lock() {
            it.clear();
        }
        if let Ok(mut it) = self.names_items.lock() {
            it.clear();
        }

        if let Ok(updatelist) = globals.get::<Table>("UPDATELIST") {
            let _ = updatelist.set("CurrentDirectoryPageNumber", 0);
        }

        if let Ok(fn_get) = globals.get::<mlua::Function>(&*on_get_name_and_link) {
            self.reset_execution_budget();
            let _: mlua::Value = fn_get.call(())?;
        }

        let raw_links = self
            .links_items
            .lock()
            .map(|i| i.clone())
            .unwrap_or_default();
        let raw_names = self
            .names_items
            .lock()
            .map(|i| i.clone())
            .unwrap_or_default();

        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (idx, name) in raw_names.into_iter().enumerate() {
            let link = raw_links.get(idx).cloned().unwrap_or_default();
            let title = name.trim().to_string();
            let link = link.trim().to_string();

            if !title.is_empty() && !link.is_empty() && seen.insert(link.clone()) {
                entries.push(CatalogEntry { title, link });
            }
        }

        let mut total_pages = None;
        if let Ok(updatelist) = globals.get::<Table>("UPDATELIST") {
            let p_num: i64 = updatelist.get("CurrentDirectoryPageNumber").unwrap_or(0);
            if p_num > 1 {
                total_pages = Some(p_num as usize);
            }
        }

        Ok((entries, total_pages))
    }

    pub fn update_catalog<F>(
        &self,
        file_path: &str,
        cancel_token: Arc<std::sync::atomic::AtomicBool>,
        mut on_batch: F,
    ) -> Result<usize>
    where
        F: FnMut(Vec<CatalogEntry>, CatalogSyncProgress) -> Result<()>,
    {
        let module_opt = self.load_file(file_path)?;
        let module = module_opt.ok_or_else(|| {
            EngineError::Custom(format!("Module at {} could not be loaded", file_path))
        })?;

        let on_get_name_and_link = module.on_get_name_and_link.clone().ok_or_else(|| {
            EngineError::Custom("Module does not implement OnGetNameAndLink".into())
        })?;

        let total_directories = module.total_directory.max(1);
        let mut total_items = 0usize;

        let globals = self.lua.globals();

        for dir_idx in 0..total_directories {
            if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            if let Ok(m_tbl) = globals.get::<Table>("MODULE") {
                let _ = m_tbl.set("CurrentDirectoryIndex", dir_idx as i64);
            }

            globals.set("PAGENUMBER", 0)?;
            let mut known_total_pages = None;
            if let Some(ref dir_fn_name) = module.on_get_directory_page_number {
                if let Ok(dir_fn) = globals.get::<mlua::Function>(&**dir_fn_name) {
                    self.reset_execution_budget();
                    if dir_fn.call::<()>(()).is_ok() {
                        let p: i64 = globals.get("PAGENUMBER").unwrap_or(0);
                        if p > 0 {
                            known_total_pages = Some(p as usize);
                        }
                    }
                }
            }

            let mut page = 0usize;
            let mut consecutive_empty = 0usize;
            let mut dir_seen_links = std::collections::HashSet::new();

            loop {
                if cancel_token.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                globals.set("URL", page as i64)?;

                if let Ok(mut it) = self.links_items.lock() {
                    it.clear();
                }
                if let Ok(mut it) = self.names_items.lock() {
                    it.clear();
                }
                if let Ok(updatelist) = globals.get::<Table>("UPDATELIST") {
                    let _ = updatelist.set("CurrentDirectoryPageNumber", 0);
                }

                if let Ok(fn_get) = globals.get::<mlua::Function>(&*on_get_name_and_link) {
                    self.reset_execution_budget();
                    let _: mlua::Value = fn_get.call(())?;
                }

                let raw_links = self
                    .links_items
                    .lock()
                    .map(|i| i.clone())
                    .unwrap_or_default();
                let raw_names = self
                    .names_items
                    .lock()
                    .map(|i| i.clone())
                    .unwrap_or_default();

                let mut entries = Vec::new();

                for (idx, name) in raw_names.into_iter().enumerate() {
                    let link = raw_links.get(idx).cloned().unwrap_or_default();
                    let title = name.trim().to_string();
                    let link = link.trim().to_string();

                    if !title.is_empty() && !link.is_empty() && dir_seen_links.insert(link.clone())
                    {
                        entries.push(CatalogEntry { title, link });
                    }
                }

                if entries.is_empty() {
                    consecutive_empty += 1;
                    if consecutive_empty >= 1 {
                        // Reached end of catalog or single-page catalog
                        break;
                    }
                } else {
                    consecutive_empty = 0;
                    total_items += entries.len();

                    let progress = CatalogSyncProgress {
                        module_id: module.id.clone(),
                        directory_index: dir_idx,
                        total_directories,
                        current_page: page,
                        total_pages: known_total_pages,
                        items_found: total_items,
                    };

                    on_batch(entries, progress)?;
                }

                if known_total_pages.is_none() {
                    if let Ok(updatelist) = globals.get::<Table>("UPDATELIST") {
                        let p_num: i64 = updatelist.get("CurrentDirectoryPageNumber").unwrap_or(0);
                        if p_num > 1 {
                            known_total_pages = Some(p_num as usize);
                        }
                    }
                }

                if let Some(max_p) = known_total_pages {
                    if page + 1 >= max_p {
                        break;
                    }
                }

                page += 1;
                if page >= 2500 {
                    break;
                }

                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }

        Ok(total_items)
    }

    pub fn modules(&self) -> Arc<Mutex<Vec<WebsiteModule>>> {
        self.modules.clone()
    }

    pub fn http_client(&self) -> &HttpClient {
        &self.http_client
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires live external network access"]
    fn test_lelscan_xpath_diagnostic() {
        let engine = LuaModuleEngine::new().unwrap();
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/LelscanFR.lua");
        let m = engine.load_file(&module_path).unwrap().unwrap();
        println!("Loaded module: {}", m.name);

        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let res = engine.update_catalog(module_path.to_str().unwrap(), cancel, |entries, prog| {
            println!(
                "Got {} entries on page {}",
                entries.len(),
                prog.current_page
            );
            for e in entries.iter().take(3) {
                println!("  entry: {} -> {}", e.title, e.link);
            }
            Ok(())
        });
        println!("update_catalog result: {:?}", res);
        assert!(res.unwrap() > 0);
    }

    #[test]
    fn test_lua_engine_init_and_js_execution() {
        let engine = LuaModuleEngine::new().unwrap();
        // Test JS unpacker / Duktape emulation via boa_engine
        let res: String = engine
            .lua
            .load(
                r#"
            return duktape.ExecJS("1 + 1")
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(res, "2");
    }

    #[test]
    fn test_load_real_fmd2_module() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/3asq.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let module_opt = engine.load_file(module_path).unwrap();
        assert!(module_opt.is_some());
        let module = module_opt.unwrap();
        assert_eq!(module.name, "3asq");
        assert_eq!(module.root_url, "https://3asq.online");
    }

    #[test]
    fn test_novelcool_fr_load() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let module_opt = engine.load_file(module_path).unwrap();
        assert!(module_opt.is_some());
        let module = module_opt.unwrap();
        assert_eq!(module.name, "NovelCoolFR");
        assert_eq!(module.root_url, "https://fr.novelcool.com");
    }

    #[tokio::test]
    #[ignore = "requires live external network access"]
    async fn test_novelcool_fr_get_info() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let res = engine.get_manga_info(
            module_path.to_str().unwrap(),
            "https://fr.novelcool.com/novel/Jujutsu-Kaisen.html",
        );

        let info = res.expect("get_manga_info on NovelCoolFR failed");
        println!("Title: {}", info.title);
        println!("Chapters count: {}", info.chapters.len());
        assert!(!info.title.is_empty());
        assert!(!info.chapters.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live external network access"]
    async fn test_novelcool_fr_get_chapter_pages() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let res = engine.get_chapter_pages(
            module_path.to_str().unwrap(),
            "https://fr.novelcool.com/chapter/Chapitre-131/5271027/",
        );

        let pages = res.expect("get_chapter_pages on NovelCoolFR failed");
        println!("Pages count: {}", pages.len());
        if let Some(p) = pages.first() {
            println!("First page URL: {}", p);
        }
        assert_eq!(pages.len(), 21);
    }

    #[test]
    #[ignore = "requires live external network access"]
    fn test_novelcool_fr_fetch_catalog_page() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let (entries, total_pages) = engine
            .fetch_catalog_page(module_path.to_str().unwrap(), 0, 0)
            .expect("fetch_catalog_page on NovelCoolFR failed");

        println!("Catalog entries on page 0: {}", entries.len());
        println!("Detected total pages: {:?}", total_pages);
        if let Some(first) = entries.first() {
            println!("First manga: {} -> {}", first.title, first.link);
        }
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_sushiscan_cover_xpath() {
        let html = r#"<div class="thumb" itemprop="image"><img src="https://sushiscan.net/wp-content/uploads/OnePieceCov.jpg" /></div>"#;
        let cover = crate::bindings::xquery::eval_xpath_string(
            html,
            "//div[@itemprop=\"image\"]//img/@src",
        );
        println!("Extracted cover: '{}'", cover);
        assert_eq!(
            cover,
            "https://sushiscan.net/wp-content/uploads/OnePieceCov.jpg"
        );
    }

    #[tokio::test]
    #[ignore = "requires live external network access"]
    async fn test_sushiscan_cover_fetch() {
        let client = fmd_net::HttpClient::new().unwrap();
        let mut headers = std::collections::HashMap::new();
        headers.insert("Referer".into(), "https://sushiscan.net/".into());
        let res = client
            .get_bytes_with_headers(
                "https://sushiscan.net/wp-content/uploads/RoadToLaughTaleCover.png",
                &headers,
            )
            .await;
        match res {
            Ok(bytes) => println!("Success fetching cover! Received {} bytes", bytes.len()),
            Err(e) => println!("Error fetching cover: {}", e),
        }
    }

    #[tokio::test]
    #[ignore = "requires live external network access"]
    async fn test_mangaplus_get_info() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/MangaPlus.lua");

        let engine = LuaModuleEngine::new().unwrap();
        let res = engine.get_manga_info(
            module_path.to_str().unwrap(),
            "https://mangaplus.shueisha.co.jp/titles/100034",
        );

        let info = res.expect("get_manga_info on MangaPlus failed");
        println!("MangaPlus Title: {}", info.title);
        println!("MangaPlus Chapters count: {}", info.chapters.len());
        println!(
            "MangaPlus Alternate versions: {:?}",
            info.alternate_versions
        );
        assert!(!info.title.is_empty());
        assert!(!info.chapters.is_empty());
        assert!(!info.alternate_versions.is_empty());
    }

    #[tokio::test]
    #[ignore = "requires live external network access"]
    async fn test_mangaplus_french_info_and_download() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let module_path =
            std::path::Path::new(&manifest_dir).join("../../lua/modules/MangaPlus.lua");

        let engine = LuaModuleEngine::new().unwrap();
        // Test French edition directly
        let res = engine.get_manga_info(
            module_path.to_str().unwrap(),
            "https://mangaplus.shueisha.co.jp/titles/700004",
        );

        let info = res.expect("get_manga_info on French MangaPlus failed");
        println!("French Title: {}", info.title);
        assert!(info.title.contains("[FR]"));
        assert!(!info.chapters.is_empty());

        let first_ch = &info.chapters[0];
        println!(
            "First French chapter: {} ({})",
            first_ch.title, first_ch.url
        );

        let pages = engine
            .get_chapter_pages(module_path.to_str().unwrap(), &first_ch.url)
            .expect("get_chapter_pages on French MangaPlus failed");
        println!("Pages count: {}", pages.len());
        assert!(!pages.is_empty());

        // Test downloading & decrypting page 1
        let first_page_url = &pages[0];
        let img_bytes = engine
            .download_custom_image(module_path.to_str().unwrap(), first_page_url, 0)
            .expect("download_image on MangaPlus failed")
            .expect("image bytes empty");

        println!(
            "Downloaded & decrypted page size: {} bytes",
            img_bytes.len()
        );
        assert!(img_bytes.len() > 1000);
        // Verify JPEG or PNG or WebP header
        let is_jpeg = img_bytes.starts_with(&[0xFF, 0xD8]);
        let is_png = img_bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]);
        let is_webp =
            img_bytes.len() >= 12 && &img_bytes[0..4] == b"RIFF" && &img_bytes[8..12] == b"WEBP";
        assert!(
            is_jpeg || is_png || is_webp,
            "Decrypted image is not a valid image format"
        );
        println!(
            "Decrypted image format verified! (JPEG: {}, PNG: {}, WebP: {})",
            is_jpeg, is_png, is_webp
        );
    }

    #[test]
    fn test_lua_sandbox_hardening() {
        let engine = LuaModuleEngine::new().expect("Failed to create LuaModuleEngine");
        let lua = &engine.lua;

        // Verify dangerous os/io/global primitives are disabled
        for expr in [
            "return os and os.execute",
            "return os and os.remove",
            "return os and os.getenv",
            "return io and io.popen",
            "return io and io.tmpfile",
            "return package and package.loadlib",
            "return load",
            "return loadfile",
            "return dofile",
            "return loadstring",
            "local pb = require('pb'); return pb.loadfile",
            "return package and package.searchers and package.searchers[3]",
            "return package and package.searchers and package.searchers[4]",
        ] {
            let val: mlua::Value = lua.load(expr).eval().unwrap();
            assert!(val.is_nil(), "Expression '{}' must evaluate to nil", expr);
        }

        // Verify safe os.time still functions normally
        let time_val: i64 = lua.load("return os.time()").eval().unwrap();
        assert!(time_val > 0, "os.time() should return valid timestamp");

        // Verify filesystem escapes across io.open / io.lines / io.input / io.output are blocked
        for expr in [
            "return io.open('/etc/passwd', 'r')",
            "return io.open('//etc/passwd', 'r')",
            "return io.open('/./etc/passwd', 'r')",
            "return io.open('../../Cargo.toml', 'r')",
            "return io.open('lua/modules/sandbox_write_test.lua', 'w')",
            "return io.lines('/etc/passwd')",
            "return io.input('/etc/passwd')",
            "return io.output('lua/modules/sandbox_write_test.lua')",
        ] {
            let res: mlua::Result<mlua::Value> = lua.load(expr).eval();
            assert!(res.is_err(), "Sandbox must block '{}'", expr);
        }

        // Verify legitimate read inside lua/ works (e.g., MangaPlus.proto)
        let proto_ok: bool = lua
            .load(
                r#"
                local f = io.open('lua/modules/MangaPlus.proto', 'rb') or io.open('../../lua/modules/MangaPlus.proto', 'rb')
                if not f then return false end
                local content = f:read('*all')
                f:close()
                return content ~= nil and #content > 0
                "#,
            )
            .eval()
            .unwrap();
        assert!(proto_ok, "Legitimate lua/modules/*.proto read must succeed");

        // Verify legitimate userdata/ write + read workflow (MangaDex mapping pattern)
        let userdata_dir = std::env::current_dir().unwrap().join("userdata");
        let userdata_existed = userdata_dir.exists();
        let mapping_rel = "userdata/_sandbox_test_mapping.txt";
        let mapping_abs = userdata_dir.join("_sandbox_test_mapping.txt");
        let rw_ok: bool = lua
            .load(format!(
                r#"
                local wf = assert(io.open('{p}', 'a'))
                io.output(wf)
                io.write('123;test\n')
                wf:close()
                local lines = {{}}
                for line in io.lines('{p}') do
                    table.insert(lines, line)
                end
                return #lines >= 1 and lines[1] == '123;test'
                "#,
                p = mapping_rel
            ))
            .eval()
            .unwrap();
        let _ = std::fs::remove_file(&mapping_abs);
        if !userdata_existed {
            let _ = std::fs::remove_dir(&userdata_dir);
        }
        assert!(
            rw_ok,
            "Legitimate userdata/ write & io.lines read must succeed"
        );
    }

    #[test]
    fn test_lua_and_js_execution_limits() {
        let engine = LuaModuleEngine::new().expect("Failed to create LuaModuleEngine");
        engine.reset_execution_budget();

        let lua_loop: mlua::Result<mlua::Value> = engine.lua.load("while true do end").eval();
        assert!(lua_loop.is_err(), "Infinite Lua loop must abort");

        let js_loop: String = engine
            .lua
            .load(r#"return duktape.ExecJS("while (true) {}")"#)
            .eval()
            .unwrap();
        assert!(
            js_loop.starts_with("Error:"),
            "Infinite JS loop must return loop-limit error, got: {}",
            js_loop
        );
    }
}
