use crate::auth::{generate_secure_token, hash_token, AuthContext, Role, User, UserPermissions};
use crate::cache::{MangaCacheItem, ModuleCachePolicy, TtlUnit};
use crate::errors::{Error, Result};
use chrono::Utc;
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    sqlite::SqlitePoolOptions,
    Pool, Postgres, Row, Sqlite,
};
use std::path::Path;
use std::str::FromStr;
use tracing::info;

#[derive(Clone)]
enum DbBackend {
    Sqlite(Pool<Sqlite>),
    Postgres {
        pool: Pool<Postgres>,
        schema: Option<String>,
    },
}

#[derive(Clone)]
pub struct Database {
    backend: DbBackend,
    database_url: String,
}

fn to_pg_sql(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;

    while let Some(c) = chars.next() {
        if c == '\'' {
            out.push(c);
            if in_single_quote && chars.peek() == Some(&'\'') {
                out.push(chars.next().unwrap());
            } else {
                in_single_quote = !in_single_quote;
            }
            continue;
        }
        if !in_single_quote && c == '?' && chars.peek().is_some_and(|n| n.is_ascii_digit()) {
            out.push('$');
            while chars.peek().is_some_and(|n| n.is_ascii_digit()) {
                out.push(chars.next().unwrap());
            }
            continue;
        }
        out.push(c);
    }
    out
}

macro_rules! db_execute {
    ($db:expr, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                sqlx::query($sql)$(.bind($arg))*.execute(pool).await.map(|r| r.rows_affected())
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                sqlx::query(&pg_sql)$(.bind($arg))*.execute(pool).await.map(|r| r.rows_affected())
            }
        }
    }};
}

macro_rules! db_fetch_all_map {
    ($db:expr, $sql:expr, [$($arg:expr),* $(,)?], |$row:ident| $body:expr) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                let rows = sqlx::query($sql)$(.bind($arg))*.fetch_all(pool).await?;
                Ok::<_, sqlx::Error>(rows.into_iter().map(|$row| $body).collect::<Vec<_>>())
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                let rows = sqlx::query(&pg_sql)$(.bind($arg))*.fetch_all(pool).await?;
                Ok::<_, sqlx::Error>(rows.into_iter().map(|$row| $body).collect::<Vec<_>>())
            }
        }
    }};
}

macro_rules! db_fetch_optional_map {
    ($db:expr, $sql:expr, [$($arg:expr),* $(,)?], |$row:ident| $body:expr) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                let row = sqlx::query($sql)$(.bind($arg))*.fetch_optional(pool).await?;
                Ok::<_, sqlx::Error>(row.map(|$row| $body))
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                let row = sqlx::query(&pg_sql)$(.bind($arg))*.fetch_optional(pool).await?;
                Ok::<_, sqlx::Error>(row.map(|$row| $body))
            }
        }
    }};
}

macro_rules! db_query_as_fetch_one {
    ($db:expr, $ty:ty, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                sqlx::query_as::<_, $ty>($sql)$(.bind($arg))*.fetch_one(pool).await
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                sqlx::query_as::<_, $ty>(&pg_sql)$(.bind($arg))*.fetch_one(pool).await
            }
        }
    }};
}

macro_rules! db_query_as_fetch_optional {
    ($db:expr, $ty:ty, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                sqlx::query_as::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(pool).await
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                sqlx::query_as::<_, $ty>(&pg_sql)$(.bind($arg))*.fetch_optional(pool).await
            }
        }
    }};
}

macro_rules! db_query_as_fetch_all {
    ($db:expr, $ty:ty, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                sqlx::query_as::<_, $ty>($sql)$(.bind($arg))*.fetch_all(pool).await
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                sqlx::query_as::<_, $ty>(&pg_sql)$(.bind($arg))*.fetch_all(pool).await
            }
        }
    }};
}

macro_rules! db_query_scalar_fetch_optional {
    ($db:expr, $ty:ty, $sql:expr $(, $arg:expr)* $(,)?) => {{
        match &$db.backend {
            DbBackend::Sqlite(pool) => {
                sqlx::query_scalar::<_, $ty>($sql)$(.bind($arg))*.fetch_optional(pool).await
            }
            DbBackend::Postgres { pool, .. } => {
                let pg_sql = to_pg_sql($sql);
                sqlx::query_scalar::<_, $ty>(&pg_sql)$(.bind($arg))*.fetch_optional(pool).await
            }
        }
    }};
}

impl Database {
    pub fn is_postgres_url(url: &str) -> bool {
        let lower = url.trim().to_ascii_lowercase();
        lower.starts_with("postgres://") || lower.starts_with("postgresql://")
    }

    pub fn resolve_url(cli_override: Option<&str>, default_sqlite_path: &str) -> String {
        if let Some(val) = cli_override.map(str::trim).filter(|s| !s.is_empty()) {
            return val.to_string();
        }
        if let Ok(env_val) = std::env::var("FMD_DATABASE_URL") {
            let trimmed = env_val.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        format!("sqlite://{}", default_sqlite_path)
    }

    pub async fn new_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .min_connections(1)
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(sqlx::sqlite::SqliteConnectOptions::new().in_memory(true))
            .await?;
        let db = Self {
            backend: DbBackend::Sqlite(pool),
            database_url: "sqlite::memory:".to_string(),
        };
        db.migrate().await?;
        Ok(db)
    }

    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_str = db_path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::Custom("Invalid database path".into()))?;

        if db_str == ":memory:" {
            return Self::new_in_memory().await;
        }

        if Self::is_postgres_url(db_str) || db_str.starts_with("sqlite://") {
            return Self::connect(db_str).await;
        }

        if let Some(parent) = db_path.as_ref().parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let connection_string = format!("sqlite://{}?mode=rwc", db_str);
        Self::connect(&connection_string).await
    }

    pub async fn connect(database_url: &str) -> Result<Self> {
        let url = database_url.trim();
        if url == ":memory:" || url == "sqlite::memory:" {
            return Self::new_in_memory().await;
        }
        if Self::is_postgres_url(url) {
            let options = PgConnectOptions::from_str(url)?;
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect_with(options)
                .await?;
            let db = Self {
                backend: DbBackend::Postgres { pool, schema: None },
                database_url: url.to_string(),
            };
            db.migrate().await?;
            return Ok(db);
        }

        let conn_str = if let Some(rest) = url.strip_prefix("sqlite://") {
            let file_part = rest.split('?').next().unwrap_or("");
            if !file_part.is_empty() && file_part != ":memory:" {
                if let Some(parent) = Path::new(file_part).parent() {
                    if !parent.as_os_str().is_empty() {
                        std::fs::create_dir_all(parent)?;
                    }
                }
            }
            if rest.contains('?') {
                url.to_string()
            } else {
                format!("{url}?mode=rwc")
            }
        } else {
            if let Some(parent) = Path::new(url).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            format!("sqlite://{url}?mode=rwc")
        };

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&conn_str)
            .await?;

        let db = Self {
            backend: DbBackend::Sqlite(pool),
            database_url: conn_str,
        };
        db.migrate().await?;
        Ok(db)
    }

    /// Creates an isolated ephemeral database for a demo visitor slot.
    /// On PostgreSQL, creates a dedicated schema (`demo_slot_<uuid>`) and sets `search_path`.
    /// On SQLite, uses an in-memory SQLite database.
    pub async fn new_demo_slot(base_url: Option<&str>) -> Result<Self> {
        let Some(url) = base_url.filter(|u| Self::is_postgres_url(u)) else {
            return Self::new_in_memory().await;
        };

        let schema = format!("demo_slot_{}", uuid::Uuid::new_v4().simple());
        let admin_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_with(PgConnectOptions::from_str(url)?)
            .await?;
        let create_stmt = format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\"");
        sqlx::query(&create_stmt).execute(&admin_pool).await?;
        admin_pool.close().await;

        let schema_for_hook = schema.clone();
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .after_connect(move |conn, _meta| {
                let schema_name = schema_for_hook.clone();
                Box::pin(async move {
                    let stmt = format!("SET search_path TO \"{schema_name}\"");
                    sqlx::query(&stmt).execute(conn).await?;
                    Ok(())
                })
            })
            .connect_with(PgConnectOptions::from_str(url)?)
            .await?;

        let db = Self {
            backend: DbBackend::Postgres {
                pool,
                schema: Some(schema),
            },
            database_url: url.to_string(),
        };
        db.migrate().await?;
        Ok(db)
    }

    pub fn is_postgres(&self) -> bool {
        matches!(self.backend, DbBackend::Postgres { .. })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn is_closed(&self) -> bool {
        match &self.backend {
            DbBackend::Sqlite(pool) => pool.is_closed(),
            DbBackend::Postgres { pool, .. } => pool.is_closed(),
        }
    }

    pub async fn close(&self) {
        match &self.backend {
            DbBackend::Sqlite(pool) => pool.close().await,
            DbBackend::Postgres { pool, schema } => {
                if let Some(schema_name) = schema {
                    let drop_stmt = format!("DROP SCHEMA IF EXISTS \"{schema_name}\" CASCADE");
                    let _ = sqlx::query(&drop_stmt).execute(pool).await;
                }
                pool.close().await;
            }
        }
    }

    pub async fn migrate(&self) -> Result<()> {
        match &self.backend {
            DbBackend::Sqlite(pool) => Self::migrate_sqlite(pool).await,
            DbBackend::Postgres { pool, .. } => Self::migrate_postgres(pool).await,
        }
    }

    async fn migrate_sqlite(pool: &Pool<Sqlite>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS mangas (
                id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                original_url TEXT NOT NULL,
                cover_url TEXT,
                author TEXT,
                artist TEXT,
                description TEXT,
                genres TEXT,
                status TEXT NOT NULL DEFAULT 'unknown',
                added_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS favorites (
                manga_id TEXT PRIMARY KEY,
                order_index INTEGER NOT NULL DEFAULT 0,
                enabled BOOLEAN NOT NULL DEFAULT 1,
                current_chapter TEXT,
                save_to TEXT,
                last_checked_at DATETIME,
                last_updated_at DATETIME,
                FOREIGN KEY (manga_id) REFERENCES mangas(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS chapters (
                id TEXT PRIMARY KEY,
                manga_id TEXT NOT NULL,
                title TEXT NOT NULL,
                chapter_number REAL NOT NULL DEFAULT 0,
                url TEXT NOT NULL,
                is_downloaded BOOLEAN NOT NULL DEFAULT 0,
                downloaded_at DATETIME,
                FOREIGN KEY (manga_id) REFERENCES mangas(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                manga_title TEXT NOT NULL,
                chapter_title TEXT NOT NULL,
                chapter_url TEXT NOT NULL,
                module_id TEXT NOT NULL,
                format TEXT NOT NULL DEFAULT 'Directory',
                total_pages INTEGER NOT NULL DEFAULT 0,
                downloaded_pages INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'Pending',
                error_message TEXT,
                retry_count INTEGER NOT NULL DEFAULT 0,
                max_retries INTEGER NOT NULL DEFAULT 3,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS catalog (
                id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                link TEXT NOT NULL,
                cover_url TEXT,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(module_id, link)
            );

            CREATE INDEX IF NOT EXISTS idx_catalog_module ON catalog(module_id);
            CREATE INDEX IF NOT EXISTS idx_catalog_title ON catalog(title);

            CREATE VIRTUAL TABLE IF NOT EXISTS catalog_fts USING fts5(
                title,
                content='catalog',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS catalog_ai AFTER INSERT ON catalog BEGIN
              INSERT INTO catalog_fts(rowid, title) VALUES (new.rowid, new.title);
            END;

            CREATE TRIGGER IF NOT EXISTS catalog_ad AFTER DELETE ON catalog BEGIN
              INSERT INTO catalog_fts(catalog_fts, rowid, title) VALUES('delete', old.rowid, old.title);
            END;

            CREATE TRIGGER IF NOT EXISTS catalog_au AFTER UPDATE ON catalog BEGIN
              INSERT INTO catalog_fts(catalog_fts, rowid, title) VALUES('delete', old.rowid, old.title);
              INSERT INTO catalog_fts(rowid, title) VALUES (new.rowid, new.title);
            END;

            CREATE TABLE IF NOT EXISTS module_sync_status (
                module_id TEXT PRIMARY KEY,
                total_mangas INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'Idle',
                last_synced_at DATETIME,
                error_message TEXT
            );

            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                sso_provider TEXT,
                sso_subject TEXT,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_login_at DATETIME,
                is_active BOOLEAN NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS user_permissions (
                user_id TEXT PRIMARY KEY,
                can_download BOOLEAN NOT NULL DEFAULT 1,
                can_manage_favorites BOOLEAN NOT NULL DEFAULT 1,
                can_trigger_catalog_sync BOOLEAN NOT NULL DEFAULT 1,
                can_force_refresh BOOLEAN NOT NULL DEFAULT 1,
                can_view_modules BOOLEAN NOT NULL DEFAULT 1,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS user_sessions (
                id TEXT PRIMARY KEY,
                token_hash TEXT UNIQUE NOT NULL,
                user_id TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at DATETIME NOT NULL,
                last_active_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                user_agent TEXT,
                ip_address TEXT,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON user_sessions(token_hash);
            CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON user_sessions(user_id);

            CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT UNIQUE NOT NULL,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at DATETIME,
                last_used_at DATETIME,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash);

            CREATE TABLE IF NOT EXISTS manga_cache (
                manga_id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT,
                metadata_json TEXT NOT NULL,
                chapters_json TEXT NOT NULL,
                cached_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at DATETIME NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_manga_cache_module ON manga_cache(module_id);

            CREATE TABLE IF NOT EXISTS module_cache_policies (
                module_id TEXT PRIMARY KEY,
                ttl_value INTEGER NOT NULL DEFAULT 24,
                ttl_unit TEXT NOT NULL DEFAULT 'hours',
                min_delay_ms INTEGER NOT NULL DEFAULT 1000,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )
        .execute(pool)
        .await?;

        let _ = sqlx::query(
            "ALTER TABLE favorites ADD COLUMN auto_download BOOLEAN NOT NULL DEFAULT 0",
        )
        .execute(pool)
        .await;
        let _ = sqlx::query(
            "ALTER TABLE favorites ADD COLUMN download_format TEXT NOT NULL DEFAULT 'CBZ'",
        )
        .execute(pool)
        .await;
        let _ = sqlx::query("ALTER TABLE favorites ADD COLUMN user_id TEXT DEFAULT NULL")
            .execute(pool)
            .await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_favorites_user ON favorites(user_id)")
            .execute(pool)
            .await;
        let _ =
            sqlx::query("ALTER TABLE downloads ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0")
                .execute(pool)
                .await;
        let _ =
            sqlx::query("ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3")
                .execute(pool)
                .await;
        let _ = sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_chapters_manga_url ON chapters(manga_id, url)",
        )
        .execute(pool)
        .await;
        let _ = sqlx::query("INSERT INTO module_cache_policies (module_id, ttl_value, ttl_unit, min_delay_ms) VALUES ('*', 24, 'hours', 1000) ON CONFLICT(module_id) DO NOTHING")
            .execute(pool)
            .await;

        Ok(())
    }

    async fn migrate_postgres(pool: &Pool<Postgres>) -> Result<()> {
        let statements = [
            r#"CREATE TABLE IF NOT EXISTS mangas (
                id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                original_url TEXT NOT NULL,
                cover_url TEXT,
                author TEXT,
                artist TEXT,
                description TEXT,
                genres TEXT,
                status TEXT NOT NULL DEFAULT 'unknown',
                added_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"#,
            r#"CREATE TABLE IF NOT EXISTS favorites (
                manga_id TEXT PRIMARY KEY,
                order_index BIGINT NOT NULL DEFAULT 0,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                current_chapter TEXT,
                save_to TEXT,
                last_checked_at TIMESTAMPTZ,
                last_updated_at TIMESTAMPTZ,
                auto_download BOOLEAN NOT NULL DEFAULT FALSE,
                download_format TEXT NOT NULL DEFAULT 'CBZ',
                user_id TEXT DEFAULT NULL,
                FOREIGN KEY (manga_id) REFERENCES mangas(id) ON DELETE CASCADE
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_favorites_user ON favorites(user_id)",
            r#"CREATE TABLE IF NOT EXISTS chapters (
                id TEXT PRIMARY KEY,
                manga_id TEXT NOT NULL,
                title TEXT NOT NULL,
                chapter_number REAL NOT NULL DEFAULT 0,
                url TEXT NOT NULL,
                is_downloaded BOOLEAN NOT NULL DEFAULT FALSE,
                downloaded_at TIMESTAMPTZ,
                FOREIGN KEY (manga_id) REFERENCES mangas(id) ON DELETE CASCADE
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_chapters_manga_url ON chapters(manga_id, url)",
            r#"CREATE TABLE IF NOT EXISTS downloads (
                id TEXT PRIMARY KEY,
                manga_title TEXT NOT NULL,
                chapter_title TEXT NOT NULL,
                chapter_url TEXT NOT NULL,
                module_id TEXT NOT NULL,
                format TEXT NOT NULL DEFAULT 'Directory',
                total_pages BIGINT NOT NULL DEFAULT 0,
                downloaded_pages BIGINT NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'Pending',
                error_message TEXT,
                retry_count BIGINT NOT NULL DEFAULT 0,
                max_retries BIGINT NOT NULL DEFAULT 3,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"#,
            r#"CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )"#,
            r#"CREATE TABLE IF NOT EXISTS catalog (
                id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                link TEXT NOT NULL,
                cover_url TEXT,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(module_id, link)
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_catalog_module ON catalog(module_id)",
            "CREATE INDEX IF NOT EXISTS idx_catalog_title ON catalog(title)",
            "CREATE INDEX IF NOT EXISTS idx_catalog_title_fts ON catalog USING GIN (to_tsvector('simple', title))",
            r#"CREATE TABLE IF NOT EXISTS module_sync_status (
                module_id TEXT PRIMARY KEY,
                total_mangas BIGINT NOT NULL DEFAULT 0,
                status TEXT NOT NULL DEFAULT 'Idle',
                last_synced_at TIMESTAMPTZ,
                error_message TEXT
            )"#,
            r#"CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                username TEXT UNIQUE NOT NULL,
                password_hash TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                sso_provider TEXT,
                sso_subject TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                last_login_at TIMESTAMPTZ,
                is_active BOOLEAN NOT NULL DEFAULT TRUE
            )"#,
            r#"CREATE TABLE IF NOT EXISTS user_permissions (
                user_id TEXT PRIMARY KEY,
                can_download BOOLEAN NOT NULL DEFAULT TRUE,
                can_manage_favorites BOOLEAN NOT NULL DEFAULT TRUE,
                can_trigger_catalog_sync BOOLEAN NOT NULL DEFAULT TRUE,
                can_force_refresh BOOLEAN NOT NULL DEFAULT TRUE,
                can_view_modules BOOLEAN NOT NULL DEFAULT TRUE,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )"#,
            r#"CREATE TABLE IF NOT EXISTS user_sessions (
                id TEXT PRIMARY KEY,
                token_hash TEXT UNIQUE NOT NULL,
                user_id TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at TIMESTAMPTZ NOT NULL,
                last_active_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                user_agent TEXT,
                ip_address TEXT,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON user_sessions(token_hash)",
            "CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON user_sessions(user_id)",
            r#"CREATE TABLE IF NOT EXISTS api_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT UNIQUE NOT NULL,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at TIMESTAMPTZ,
                last_used_at TIMESTAMPTZ,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_api_tokens_hash ON api_tokens(token_hash)",
            r#"CREATE TABLE IF NOT EXISTS manga_cache (
                manga_id TEXT PRIMARY KEY,
                module_id TEXT NOT NULL,
                title TEXT NOT NULL,
                cover_url TEXT,
                metadata_json TEXT NOT NULL,
                chapters_json TEXT NOT NULL,
                cached_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
                expires_at TIMESTAMPTZ NOT NULL
            )"#,
            "CREATE INDEX IF NOT EXISTS idx_manga_cache_module ON manga_cache(module_id)",
            r#"CREATE TABLE IF NOT EXISTS module_cache_policies (
                module_id TEXT PRIMARY KEY,
                ttl_value BIGINT NOT NULL DEFAULT 24,
                ttl_unit TEXT NOT NULL DEFAULT 'hours',
                min_delay_ms BIGINT NOT NULL DEFAULT 1000,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
            )"#,
            "INSERT INTO module_cache_policies (module_id, ttl_value, ttl_unit, min_delay_ms) VALUES ('*', 24, 'hours', 1000) ON CONFLICT(module_id) DO NOTHING",
        ];

        for stmt in statements {
            sqlx::query(stmt).execute(pool).await?;
        }
        Ok(())
    }

    /// Import legacy FMD2 userdata SQLite database
    pub async fn import_legacy_db<P: AsRef<Path>>(&self, legacy_path: P) -> Result<usize> {
        let path_str = legacy_path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::Custom("Invalid legacy path".into()))?;

        let conn_str = format!("sqlite://{}?mode=ro", path_str);
        let legacy_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&conn_str)
            .await?;

        let mut count = 0;

        // 1. Import favorites
        let rows = sqlx::query("SELECT id, \"order\", enabled, moduleid, link, title, status, currentchapter, saveto, dateadded, datelastchecked, datelastupdated FROM favorites")
            .fetch_all(&legacy_pool)
            .await;

        if let Ok(favs) = rows {
            for row in favs {
                let legacy_id: String = row.try_get("id").unwrap_or_default();
                let module_id: String = row.try_get("moduleid").unwrap_or_default();
                let link: String = row.try_get("link").unwrap_or_default();
                let title: String = row.try_get("title").unwrap_or_default();
                let current_chapter: Option<String> = row.try_get("currentchapter").ok();
                let save_to: Option<String> = row.try_get("saveto").ok();
                let order_index: i64 = row.try_get("order").unwrap_or(0);
                let enabled: bool = row.try_get("enabled").unwrap_or(true);

                let manga_id = if legacy_id.is_empty() {
                    format!("{}:{}", module_id, link)
                } else {
                    legacy_id
                };

                // Insert into mangas
                db_execute!(
                    self,
                    r#"
                    INSERT INTO mangas (id, module_id, title, original_url, status)
                    VALUES (?1, ?2, ?3, ?4, 'ongoing')
                    ON CONFLICT(id) DO UPDATE SET
                        module_id = excluded.module_id,
                        title = excluded.title,
                        original_url = excluded.original_url,
                        status = excluded.status
                    "#,
                    &manga_id,
                    &module_id,
                    &title,
                    &link,
                )?;

                // Insert into favorites
                db_execute!(
                    self,
                    r#"
                    INSERT INTO favorites (manga_id, order_index, enabled, current_chapter, save_to)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    ON CONFLICT(manga_id) DO UPDATE SET
                        order_index = excluded.order_index,
                        enabled = excluded.enabled,
                        current_chapter = excluded.current_chapter,
                        save_to = excluded.save_to
                    "#,
                    &manga_id,
                    order_index,
                    enabled,
                    current_chapter,
                    save_to,
                )?;

                count += 1;
            }
        }

        // 2. Import downloaded chapters
        let downloaded_rows = sqlx::query("SELECT id, chapters FROM downloadedchapters")
            .fetch_all(&legacy_pool)
            .await;

        if let Ok(dl_rows) = downloaded_rows {
            for row in dl_rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let chapters: String = row.try_get("chapters").unwrap_or_default();

                for line in chapters.lines() {
                    let ch_title = line.trim();
                    if !ch_title.is_empty() {
                        let ch_id = format!("{}:{}", id, ch_title);
                        let _ = db_execute!(
                            self,
                            r#"
                            INSERT INTO chapters (id, manga_id, title, chapter_number, url, is_downloaded)
                            VALUES (?1, ?2, ?3, 0.0, '', TRUE)
                            ON CONFLICT(id) DO NOTHING
                            "#,
                            &ch_id,
                            &id,
                            ch_title,
                        );
                    }
                }
            }
        }

        info!("Imported {} favorites from legacy FMD2 database", count);
        Ok(count)
    }

    pub async fn upsert_catalog_items(
        &self,
        module_id: &str,
        items: &[(String, String)], // (title, link)
    ) -> Result<usize> {
        let mut inserted = 0;
        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                for (title, link) in items {
                    let id = format!("{}:{}", module_id, link);
                    let res = sqlx::query(
                        r#"
                        INSERT INTO catalog (id, module_id, title, link, updated_at)
                        VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
                        ON CONFLICT(module_id, link) DO UPDATE SET
                            title = excluded.title,
                            updated_at = CURRENT_TIMESTAMP
                        "#,
                    )
                    .bind(&id)
                    .bind(module_id)
                    .bind(title)
                    .bind(link)
                    .execute(&mut *tx)
                    .await?;
                    if res.rows_affected() > 0 {
                        inserted += 1;
                    }
                }
                tx.commit().await?;
            }
            DbBackend::Postgres { pool, .. } => {
                let mut tx = pool.begin().await?;
                for (title, link) in items {
                    let id = format!("{}:{}", module_id, link);
                    let res = sqlx::query(
                        r#"
                        INSERT INTO catalog (id, module_id, title, link, updated_at)
                        VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
                        ON CONFLICT(module_id, link) DO UPDATE SET
                            title = excluded.title,
                            updated_at = CURRENT_TIMESTAMP
                        "#,
                    )
                    .bind(&id)
                    .bind(module_id)
                    .bind(title)
                    .bind(link)
                    .execute(&mut *tx)
                    .await?;
                    if res.rows_affected() > 0 {
                        inserted += 1;
                    }
                }
                tx.commit().await?;
            }
        }
        Ok(inserted)
    }

    pub async fn search_catalog(
        &self,
        query: &str,
        module_id: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<crate::models::CatalogItem>, i64)> {
        let q = query.trim();
        let limit = if limit <= 0 { 50 } else { limit.min(200) };
        let offset = if offset < 0 { 0 } else { offset };

        if q.is_empty() {
            // No search query: browse by module or all
            let total_row: (i64,) = if let Some(m_id) = module_id {
                db_query_as_fetch_one!(
                    self,
                    (i64,),
                    "SELECT COUNT(*) FROM catalog WHERE module_id = ?1",
                    m_id
                )?
            } else {
                db_query_as_fetch_one!(self, (i64,), "SELECT COUNT(*) FROM catalog")?
            };

            let items = if let Some(m_id) = module_id {
                db_fetch_all_map!(
                    self,
                    r#"
                    SELECT id, module_id, title, link, cover_url, updated_at
                    FROM catalog
                    WHERE module_id = ?1
                    ORDER BY title ASC
                    LIMIT ?2 OFFSET ?3
                    "#,
                    [m_id, limit, offset],
                    |r| crate::models::CatalogItem {
                        id: r.get("id"),
                        module_id: r.get("module_id"),
                        title: r.get("title"),
                        link: r.get("link"),
                        cover_url: r.get("cover_url"),
                        updated_at: r.get("updated_at"),
                    }
                )?
            } else {
                db_fetch_all_map!(
                    self,
                    r#"
                    SELECT id, module_id, title, link, cover_url, updated_at
                    FROM catalog
                    ORDER BY title ASC
                    LIMIT ?1 OFFSET ?2
                    "#,
                    [limit, offset],
                    |r| crate::models::CatalogItem {
                        id: r.get("id"),
                        module_id: r.get("module_id"),
                        title: r.get("title"),
                        link: r.get("link"),
                        cover_url: r.get("cover_url"),
                        updated_at: r.get("updated_at"),
                    }
                )?
            };

            return Ok((items, total_row.0));
        }

        // Full-text search: FTS5 on SQLite, GIN tsvector on PostgreSQL, with LIKE/ILIKE fallback
        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let fts_query = q
                    .split_whitespace()
                    .map(|word| {
                        let clean: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
                        if clean.is_empty() {
                            String::new()
                        } else {
                            format!("\"{}\"*", clean)
                        }
                    })
                    .filter(|w| !w.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");

                let fts_res = if !fts_query.is_empty() {
                    if let Some(m_id) = module_id {
                        sqlx::query(
                            r#"
                            SELECT c.id, c.module_id, c.title, c.link, c.cover_url, c.updated_at
                            FROM catalog_fts f
                            JOIN catalog c ON c.rowid = f.rowid
                            WHERE catalog_fts MATCH ?1 AND c.module_id = ?2
                            ORDER BY rank
                            LIMIT ?3 OFFSET ?4
                            "#,
                        )
                        .bind(&fts_query)
                        .bind(m_id)
                        .bind(limit)
                        .bind(offset)
                        .fetch_all(pool)
                        .await
                    } else {
                        sqlx::query(
                            r#"
                            SELECT c.id, c.module_id, c.title, c.link, c.cover_url, c.updated_at
                            FROM catalog_fts f
                            JOIN catalog c ON c.rowid = f.rowid
                            WHERE catalog_fts MATCH ?1
                            ORDER BY rank
                            LIMIT ?2 OFFSET ?3
                            "#,
                        )
                        .bind(&fts_query)
                        .bind(limit)
                        .bind(offset)
                        .fetch_all(pool)
                        .await
                    }
                } else {
                    Err(sqlx::Error::RowNotFound)
                };

                if let Ok(rows) = fts_res {
                    let total_count: (i64,) = if let Some(m_id) = module_id {
                        sqlx::query_as(
                            r#"
                            SELECT COUNT(*)
                            FROM catalog_fts f
                            JOIN catalog c ON c.rowid = f.rowid
                            WHERE catalog_fts MATCH ?1 AND c.module_id = ?2
                            "#,
                        )
                        .bind(&fts_query)
                        .bind(m_id)
                        .fetch_one(pool)
                        .await
                        .unwrap_or((rows.len() as i64,))
                    } else {
                        sqlx::query_as(
                            r#"
                            SELECT COUNT(*)
                            FROM catalog_fts f
                            WHERE catalog_fts MATCH ?1
                            "#,
                        )
                        .bind(&fts_query)
                        .fetch_one(pool)
                        .await
                        .unwrap_or((rows.len() as i64,))
                    };

                    let items = rows
                        .into_iter()
                        .map(|r| crate::models::CatalogItem {
                            id: r.get("id"),
                            module_id: r.get("module_id"),
                            title: r.get("title"),
                            link: r.get("link"),
                            cover_url: r.get("cover_url"),
                            updated_at: r.get("updated_at"),
                        })
                        .collect();

                    return Ok((items, total_count.0));
                }
            }
            DbBackend::Postgres { pool, .. } => {
                let ts_res = if let Some(m_id) = module_id {
                    sqlx::query(
                        r#"
                        SELECT id, module_id, title, link, cover_url, updated_at
                        FROM catalog
                        WHERE to_tsvector('simple', title) @@ plainto_tsquery('simple', $1)
                          AND module_id = $2
                        ORDER BY ts_rank(to_tsvector('simple', title), plainto_tsquery('simple', $1)) DESC, title ASC
                        LIMIT $3 OFFSET $4
                        "#,
                    )
                    .bind(q)
                    .bind(m_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(pool)
                    .await
                } else {
                    sqlx::query(
                        r#"
                        SELECT id, module_id, title, link, cover_url, updated_at
                        FROM catalog
                        WHERE to_tsvector('simple', title) @@ plainto_tsquery('simple', $1)
                        ORDER BY ts_rank(to_tsvector('simple', title), plainto_tsquery('simple', $1)) DESC, title ASC
                        LIMIT $2 OFFSET $3
                        "#,
                    )
                    .bind(q)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(pool)
                    .await
                };

                if let Ok(rows) = ts_res {
                    if !rows.is_empty() {
                        let total_count: (i64,) = if let Some(m_id) = module_id {
                            sqlx::query_as(
                                r#"
                                SELECT COUNT(*)
                                FROM catalog
                                WHERE to_tsvector('simple', title) @@ plainto_tsquery('simple', $1)
                                  AND module_id = $2
                                "#,
                            )
                            .bind(q)
                            .bind(m_id)
                            .fetch_one(pool)
                            .await
                            .unwrap_or((rows.len() as i64,))
                        } else {
                            sqlx::query_as(
                                r#"
                                SELECT COUNT(*)
                                FROM catalog
                                WHERE to_tsvector('simple', title) @@ plainto_tsquery('simple', $1)
                                "#,
                            )
                            .bind(q)
                            .fetch_one(pool)
                            .await
                            .unwrap_or((rows.len() as i64,))
                        };

                        let items = rows
                            .into_iter()
                            .map(|r| crate::models::CatalogItem {
                                id: r.get("id"),
                                module_id: r.get("module_id"),
                                title: r.get("title"),
                                link: r.get("link"),
                                cover_url: r.get("cover_url"),
                                updated_at: r.get("updated_at"),
                            })
                            .collect();

                        return Ok((items, total_count.0));
                    }
                }
            }
        }

        // Substring LIKE / ILIKE fallback
        let like_pat = format!("%{}%", q);
        let like_op = if self.is_postgres() { "ILIKE" } else { "LIKE" };
        let count_sql_mod =
            format!("SELECT COUNT(*) FROM catalog WHERE module_id = ?1 AND title {like_op} ?2");
        let count_sql_all = format!("SELECT COUNT(*) FROM catalog WHERE title {like_op} ?1");
        let select_sql_mod = format!(
            "SELECT id, module_id, title, link, cover_url, updated_at FROM catalog WHERE module_id = ?1 AND title {like_op} ?2 ORDER BY title ASC LIMIT ?3 OFFSET ?4"
        );
        let select_sql_all = format!(
            "SELECT id, module_id, title, link, cover_url, updated_at FROM catalog WHERE title {like_op} ?1 ORDER BY title ASC LIMIT ?2 OFFSET ?3"
        );

        let total_row: (i64,) = if let Some(m_id) = module_id {
            db_query_as_fetch_one!(self, (i64,), &count_sql_mod, m_id, &like_pat)?
        } else {
            db_query_as_fetch_one!(self, (i64,), &count_sql_all, &like_pat)?
        };

        let items = if let Some(m_id) = module_id {
            db_fetch_all_map!(
                self,
                &select_sql_mod,
                [m_id, &like_pat, limit, offset],
                |r| crate::models::CatalogItem {
                    id: r.get("id"),
                    module_id: r.get("module_id"),
                    title: r.get("title"),
                    link: r.get("link"),
                    cover_url: r.get("cover_url"),
                    updated_at: r.get("updated_at"),
                }
            )?
        } else {
            db_fetch_all_map!(self, &select_sql_all, [&like_pat, limit, offset], |r| {
                crate::models::CatalogItem {
                    id: r.get("id"),
                    module_id: r.get("module_id"),
                    title: r.get("title"),
                    link: r.get("link"),
                    cover_url: r.get("cover_url"),
                    updated_at: r.get("updated_at"),
                }
            })?
        };

        Ok((items, total_row.0))
    }

    pub async fn get_module_sync_status(
        &self,
        module_id: &str,
    ) -> Result<Option<crate::models::ModuleSyncStatus>> {
        let item = db_fetch_optional_map!(
            self,
            "SELECT module_id, total_mangas, status, last_synced_at, error_message FROM module_sync_status WHERE module_id = ?1",
            [module_id],
            |r| crate::models::ModuleSyncStatus {
                module_id: r.get("module_id"),
                total_mangas: r.get("total_mangas"),
                status: r.get("status"),
                last_synced_at: r.get("last_synced_at"),
                error_message: r.get("error_message"),
            }
        )?;
        Ok(item)
    }

    pub async fn set_module_sync_status(
        &self,
        module_id: &str,
        status: &str,
        error_message: Option<&str>,
        total_mangas: Option<i64>,
    ) -> Result<()> {
        let total = total_mangas.unwrap_or(0);
        db_execute!(
            self,
            r#"
            INSERT INTO module_sync_status (module_id, total_mangas, status, last_synced_at, error_message)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, ?4)
            ON CONFLICT(module_id) DO UPDATE SET
                status = excluded.status,
                last_synced_at = CURRENT_TIMESTAMP,
                error_message = excluded.error_message,
                total_mangas = CASE WHEN excluded.total_mangas > 0 THEN excluded.total_mangas ELSE module_sync_status.total_mangas END
            "#,
            module_id,
            total,
            status,
            error_message,
        )?;

        Ok(())
    }

    pub async fn list_all_sync_statuses(&self) -> Result<Vec<crate::models::ModuleSyncStatus>> {
        let list = db_fetch_all_map!(
            self,
            "SELECT module_id, total_mangas, status, last_synced_at, error_message FROM module_sync_status",
            [],
            |r| crate::models::ModuleSyncStatus {
                module_id: r.get("module_id"),
                total_mangas: r.get("total_mangas"),
                status: r.get("status"),
                last_synced_at: r.get("last_synced_at"),
                error_message: r.get("error_message"),
            }
        )?;
        Ok(list)
    }

    pub async fn get_catalog_total_count(&self) -> Result<i64> {
        let row: (i64,) = db_query_as_fetch_one!(self, (i64,), "SELECT COUNT(*) FROM catalog")?;
        Ok(row.0)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_manga_metadata(
        &self,
        manga_id: &str,
        module_id: &str,
        title: &str,
        original_url: &str,
        cover_url: Option<&str>,
        author: Option<&str>,
        description: Option<&str>,
        genres: Option<&[String]>,
    ) -> Result<()> {
        let genres_json = genres.map(|g| serde_json::to_string(g).unwrap_or_default());
        db_execute!(
            self,
            r#"
            INSERT INTO mangas (id, module_id, title, original_url, cover_url, author, description, genres, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                original_url = excluded.original_url,
                cover_url = COALESCE(excluded.cover_url, mangas.cover_url),
                author = COALESCE(excluded.author, mangas.author),
                description = COALESCE(excluded.description, mangas.description),
                genres = COALESCE(excluded.genres, mangas.genres),
                updated_at = CURRENT_TIMESTAMP
            "#,
            manga_id,
            module_id,
            title,
            original_url,
            cover_url,
            author,
            description,
            genres_json,
        )?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_or_update_favorite(
        &self,
        manga_id: &str,
        module_id: &str,
        title: &str,
        original_url: &str,
        cover_url: Option<&str>,
        auto_download: bool,
        download_format: &str,
    ) -> Result<()> {
        self.add_or_update_favorite_with_meta(
            manga_id,
            module_id,
            title,
            original_url,
            cover_url,
            None,
            None,
            None,
            auto_download,
            download_format,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_or_update_favorite_with_meta(
        &self,
        manga_id: &str,
        module_id: &str,
        title: &str,
        original_url: &str,
        cover_url: Option<&str>,
        author: Option<&str>,
        description: Option<&str>,
        genres: Option<&[String]>,
        auto_download: bool,
        download_format: &str,
    ) -> Result<()> {
        self.add_or_update_favorite_with_meta_for_user(
            None,
            manga_id,
            module_id,
            title,
            original_url,
            cover_url,
            author,
            description,
            genres,
            auto_download,
            download_format,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add_or_update_favorite_with_meta_for_user(
        &self,
        user_id: Option<&str>,
        manga_id: &str,
        module_id: &str,
        title: &str,
        original_url: &str,
        cover_url: Option<&str>,
        author: Option<&str>,
        description: Option<&str>,
        genres: Option<&[String]>,
        auto_download: bool,
        download_format: &str,
    ) -> Result<()> {
        let genres_json = genres.map(|g| serde_json::to_string(g).unwrap_or_default());
        let manga_sql = r#"
            INSERT INTO mangas (id, module_id, title, original_url, cover_url, author, description, genres, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                original_url = excluded.original_url,
                cover_url = COALESCE(excluded.cover_url, mangas.cover_url),
                author = COALESCE(excluded.author, mangas.author),
                description = COALESCE(excluded.description, mangas.description),
                genres = COALESCE(excluded.genres, mangas.genres),
                updated_at = CURRENT_TIMESTAMP
            "#;
        let fav_sql = r#"
            INSERT INTO favorites (manga_id, user_id, enabled, auto_download, download_format, last_checked_at, last_updated_at)
            VALUES (?1, ?2, TRUE, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(manga_id) DO UPDATE SET
                user_id = COALESCE(favorites.user_id, excluded.user_id),
                auto_download = excluded.auto_download,
                download_format = excluded.download_format
            "#;

        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query(manga_sql)
                    .bind(manga_id)
                    .bind(module_id)
                    .bind(title)
                    .bind(original_url)
                    .bind(cover_url)
                    .bind(author)
                    .bind(description)
                    .bind(&genres_json)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(fav_sql)
                    .bind(manga_id)
                    .bind(user_id)
                    .bind(auto_download)
                    .bind(download_format)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
            DbBackend::Postgres { pool, .. } => {
                let mut tx = pool.begin().await?;
                sqlx::query(&to_pg_sql(manga_sql))
                    .bind(manga_id)
                    .bind(module_id)
                    .bind(title)
                    .bind(original_url)
                    .bind(cover_url)
                    .bind(author)
                    .bind(description)
                    .bind(&genres_json)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(&to_pg_sql(fav_sql))
                    .bind(manga_id)
                    .bind(user_id)
                    .bind(auto_download)
                    .bind(download_format)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
        }
        Ok(())
    }

    pub async fn record_chapters(
        &self,
        manga_id: &str,
        chapters: &[crate::models::ChapterItem],
        mark_as_downloaded: bool,
    ) -> Result<usize> {
        let mut count = 0;
        let sql = r#"
            INSERT INTO chapters (id, manga_id, title, url, is_downloaded, downloaded_at)
            VALUES (?1, ?2, ?3, ?4, ?5, CASE WHEN ?5 THEN CURRENT_TIMESTAMP ELSE NULL END)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                is_downloaded = CASE WHEN excluded.is_downloaded THEN TRUE ELSE chapters.is_downloaded END
            "#;
        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                for ch in chapters {
                    let ch_id = format!("{}:{}", manga_id, ch.url);
                    let res = sqlx::query(sql)
                        .bind(&ch_id)
                        .bind(manga_id)
                        .bind(&ch.title)
                        .bind(&ch.url)
                        .bind(mark_as_downloaded)
                        .execute(&mut *tx)
                        .await?;
                    if res.rows_affected() > 0 {
                        count += 1;
                    }
                }
                tx.commit().await?;
            }
            DbBackend::Postgres { pool, .. } => {
                let mut tx = pool.begin().await?;
                let pg_sql = to_pg_sql(sql);
                for ch in chapters {
                    let ch_id = format!("{}:{}", manga_id, ch.url);
                    let res = sqlx::query(&pg_sql)
                        .bind(&ch_id)
                        .bind(manga_id)
                        .bind(&ch.title)
                        .bind(&ch.url)
                        .bind(mark_as_downloaded)
                        .execute(&mut *tx)
                        .await?;
                    if res.rows_affected() > 0 {
                        count += 1;
                    }
                }
                tx.commit().await?;
            }
        }
        Ok(count)
    }

    pub async fn check_favorite_new_chapters(
        &self,
        manga_id: &str,
        fetched_chapters: &[crate::models::ChapterItem],
    ) -> Result<Vec<crate::models::ChapterItem>> {
        let urls = db_fetch_all_map!(
            self,
            "SELECT url FROM chapters WHERE manga_id = ?1",
            [manga_id],
            |r| r.get::<String, _>("url")
        )?;
        let known_urls: std::collections::HashSet<String> = urls.into_iter().collect();

        let mut new_chapters = Vec::new();
        for ch in fetched_chapters {
            if !known_urls.contains(&ch.url) {
                new_chapters.push(ch.clone());
            }
        }

        if !new_chapters.is_empty() {
            self.record_chapters(manga_id, &new_chapters, false).await?;
            db_execute!(
                self,
                "UPDATE favorites SET last_checked_at = CURRENT_TIMESTAMP, last_updated_at = CURRENT_TIMESTAMP WHERE manga_id = ?1",
                manga_id,
            )?;
        } else {
            db_execute!(
                self,
                "UPDATE favorites SET last_checked_at = CURRENT_TIMESTAMP WHERE manga_id = ?1",
                manga_id,
            )?;
        }

        Ok(new_chapters)
    }

    pub async fn mark_chapter_downloaded(&self, manga_id: &str, chapter_url: &str) -> Result<()> {
        let ch_id = format!("{}:{}", manga_id, chapter_url);
        db_execute!(
            self,
            r#"
            UPDATE chapters
            SET is_downloaded = TRUE, downloaded_at = CURRENT_TIMESTAMP
            WHERE (id = ?1 AND ?1 != '') OR (manga_id = ?2 AND url = ?3) OR url = ?3
            "#,
            &ch_id,
            manga_id,
            chapter_url,
        )?;

        Ok(())
    }

    pub async fn reconcile_downloaded_chapters(&self) -> Result<usize> {
        let affected = db_execute!(
            self,
            r#"
            UPDATE chapters
            SET is_downloaded = FALSE, downloaded_at = NULL
            WHERE is_downloaded = TRUE
              AND url NOT IN (SELECT chapter_url FROM downloads WHERE status = 'Completed')
            "#,
        )?;

        Ok(affected as usize)
    }

    pub async fn list_favorites_with_stats(&self) -> Result<Vec<crate::models::Favorite>> {
        self.list_favorites_for_user(None).await
    }

    pub async fn list_favorites_for_user(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<crate::models::Favorite>> {
        let favs = match user_id {
            Some(uid) => db_fetch_all_map!(
                self,
                r#"
                SELECT
                    f.manga_id,
                    m.title,
                    m.module_id,
                    m.original_url,
                    m.cover_url,
                    f.order_index,
                    f.enabled,
                    COALESCE(f.auto_download, FALSE) as auto_download,
                    COALESCE(f.download_format, 'CBZ') as download_format,
                    f.current_chapter,
                    f.save_to,
                    f.last_checked_at,
                    f.last_updated_at,
                    COUNT(c.id) as total_chapters,
                    COUNT(CASE WHEN c.is_downloaded THEN 1 END) as downloaded_count
                FROM favorites f
                JOIN mangas m ON m.id = f.manga_id
                LEFT JOIN chapters c ON c.manga_id = f.manga_id
                WHERE f.user_id = ?1 OR f.user_id IS NULL
                GROUP BY f.manga_id, m.id
                ORDER BY f.order_index, f.last_updated_at DESC
                "#,
                [uid],
                |r| crate::models::Favorite {
                    manga_id: r.get("manga_id"),
                    title: r.get("title"),
                    module_id: r.get("module_id"),
                    original_url: r.get("original_url"),
                    cover_url: r.get("cover_url"),
                    order_index: r.get("order_index"),
                    enabled: r.get("enabled"),
                    auto_download: r.get("auto_download"),
                    download_format: r.get("download_format"),
                    current_chapter: r.get("current_chapter"),
                    save_to: r.get("save_to"),
                    last_checked_at: r.get("last_checked_at"),
                    last_updated_at: r.get("last_updated_at"),
                    total_chapters: r.get("total_chapters"),
                    downloaded_chapters_count: r.get("downloaded_count"),
                }
            )?,
            None => db_fetch_all_map!(
                self,
                r#"
                SELECT
                    f.manga_id,
                    m.title,
                    m.module_id,
                    m.original_url,
                    m.cover_url,
                    f.order_index,
                    f.enabled,
                    COALESCE(f.auto_download, FALSE) as auto_download,
                    COALESCE(f.download_format, 'CBZ') as download_format,
                    f.current_chapter,
                    f.save_to,
                    f.last_checked_at,
                    f.last_updated_at,
                    COUNT(c.id) as total_chapters,
                    COUNT(CASE WHEN c.is_downloaded THEN 1 END) as downloaded_count
                FROM favorites f
                JOIN mangas m ON m.id = f.manga_id
                LEFT JOIN chapters c ON c.manga_id = f.manga_id
                GROUP BY f.manga_id, m.id
                ORDER BY f.order_index, f.last_updated_at DESC
                "#,
                [],
                |r| crate::models::Favorite {
                    manga_id: r.get("manga_id"),
                    title: r.get("title"),
                    module_id: r.get("module_id"),
                    original_url: r.get("original_url"),
                    cover_url: r.get("cover_url"),
                    order_index: r.get("order_index"),
                    enabled: r.get("enabled"),
                    auto_download: r.get("auto_download"),
                    download_format: r.get("download_format"),
                    current_chapter: r.get("current_chapter"),
                    save_to: r.get("save_to"),
                    last_checked_at: r.get("last_checked_at"),
                    last_updated_at: r.get("last_updated_at"),
                    total_chapters: r.get("total_chapters"),
                    downloaded_chapters_count: r.get("downloaded_count"),
                }
            )?,
        };

        Ok(favs)
    }

    pub async fn update_favorite_settings(
        &self,
        manga_id: &str,
        enabled: bool,
        auto_download: bool,
        download_format: &str,
    ) -> Result<()> {
        db_execute!(
            self,
            "UPDATE favorites SET enabled = ?1, auto_download = ?2, download_format = ?3 WHERE manga_id = ?4",
            enabled,
            auto_download,
            download_format,
            manga_id,
        )?;

        Ok(())
    }

    pub async fn list_manga_chapters(&self, manga_id: &str) -> Result<Vec<(String, String, bool)>> {
        let list = db_fetch_all_map!(
            self,
            "SELECT title, url, is_downloaded FROM chapters WHERE manga_id = ?1 ORDER BY chapter_number ASC, id ASC",
            [manga_id],
            |r| (r.get("title"), r.get("url"), r.get("is_downloaded"))
        )?;
        Ok(list)
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let val = db_fetch_optional_map!(
            self,
            "SELECT value FROM settings WHERE key = ?1",
            [key],
            |r| r.get::<String, _>("value")
        )?;
        Ok(val)
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        db_execute!(
            self,
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            key,
            value,
        )?;
        Ok(())
    }

    pub async fn get_settings(&self) -> Result<crate::models::AppSettings> {
        let mut s = crate::models::AppSettings::default();
        let rows = db_fetch_all_map!(self, "SELECT key, value FROM settings", [], |r| (
            r.get::<String, _>("key"),
            r.get::<String, _>("value")
        ))?;

        for (k, v) in rows {
            match k.as_str() {
                "download_dir" => s.download_dir = v,
                "default_format" => s.default_format = v,
                "naming_template" => s.naming_template = v,
                "flaresolverr_url" => s.flaresolverr_url = v,
                "check_interval_hours" => {
                    if let Ok(n) = v.parse() {
                        s.check_interval_hours = n;
                    }
                }
                "catalog_resync_days" => {
                    if let Ok(n) = v.parse() {
                        s.catalog_resync_days = n;
                    }
                }
                _ => {}
            }
        }
        Ok(s)
    }

    pub async fn update_settings(&self, settings: &crate::models::AppSettings) -> Result<()> {
        self.set_setting("download_dir", &settings.download_dir)
            .await?;
        self.set_setting("default_format", &settings.default_format)
            .await?;
        self.set_setting("naming_template", &settings.naming_template)
            .await?;
        self.set_setting("flaresolverr_url", &settings.flaresolverr_url)
            .await?;
        self.set_setting(
            "check_interval_hours",
            &settings.check_interval_hours.to_string(),
        )
        .await?;
        self.set_setting(
            "catalog_resync_days",
            &settings.catalog_resync_days.to_string(),
        )
        .await?;
        Ok(())
    }

    // ==========================================
    // USER MANAGEMENT & AUTHENTICATION
    // ==========================================

    pub async fn count_users(&self) -> Result<i64> {
        let row: (i64,) = db_query_as_fetch_one!(self, (i64,), "SELECT count(*) FROM users")?;
        Ok(row.0)
    }

    pub async fn create_initial_admin(&self, username: &str, password: &str) -> Result<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let pwd_hash = crate::auth::hash_password(password)?;
        let now = Utc::now();

        let user_sql = r#"
            INSERT INTO users (id, username, password_hash, role, created_at, updated_at, is_active)
            VALUES (?1, ?2, ?3, 'admin', ?4, ?4, TRUE)
            "#;
        let perms_sql = r#"
            INSERT INTO user_permissions (user_id, can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules)
            VALUES (?1, TRUE, TRUE, TRUE, TRUE, TRUE)
            "#;
        let fav_sql = "UPDATE favorites SET user_id = ?1 WHERE user_id IS NULL";

        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query(user_sql)
                    .bind(&id)
                    .bind(username)
                    .bind(&pwd_hash)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(perms_sql).bind(&id).execute(&mut *tx).await?;
                sqlx::query(fav_sql).bind(&id).execute(&mut *tx).await?;
                tx.commit().await?;
            }
            DbBackend::Postgres { pool, .. } => {
                let mut tx = pool.begin().await?;
                sqlx::query(&to_pg_sql(user_sql))
                    .bind(&id)
                    .bind(username)
                    .bind(&pwd_hash)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(&to_pg_sql(perms_sql))
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(&to_pg_sql(fav_sql))
                    .bind(&id)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
        }

        Ok(User {
            id,
            username: username.to_string(),
            password_hash: Some(pwd_hash),
            role: Role::Admin,
            sso_provider: None,
            sso_subject: None,
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
        })
    }

    pub async fn create_user(
        &self,
        username: &str,
        password: Option<&str>,
        role: Role,
        permissions: Option<UserPermissions>,
        sso_provider: Option<&str>,
        sso_subject: Option<&str>,
    ) -> Result<User> {
        let id = uuid::Uuid::new_v4().to_string();
        let pwd_hash = match password {
            Some(p) => Some(crate::auth::hash_password(p)?),
            None => None,
        };
        let now = Utc::now();
        let role_str = role.as_str();
        let perms = permissions.unwrap_or_else(|| {
            if role == Role::Admin {
                UserPermissions::admin_defaults(&id)
            } else {
                UserPermissions::user_defaults(&id)
            }
        });

        let user_sql = r#"
            INSERT INTO users (id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, is_active)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, TRUE)
            "#;
        let perms_sql = r#"
            INSERT INTO user_permissions (user_id, can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#;

        match &self.backend {
            DbBackend::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                sqlx::query(user_sql)
                    .bind(&id)
                    .bind(username)
                    .bind(&pwd_hash)
                    .bind(role_str)
                    .bind(sso_provider)
                    .bind(sso_subject)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(perms_sql)
                    .bind(&id)
                    .bind(perms.can_download)
                    .bind(perms.can_manage_favorites)
                    .bind(perms.can_trigger_catalog_sync)
                    .bind(perms.can_force_refresh)
                    .bind(perms.can_view_modules)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
            DbBackend::Postgres { pool, .. } => {
                let mut tx = pool.begin().await?;
                sqlx::query(&to_pg_sql(user_sql))
                    .bind(&id)
                    .bind(username)
                    .bind(&pwd_hash)
                    .bind(role_str)
                    .bind(sso_provider)
                    .bind(sso_subject)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(&to_pg_sql(perms_sql))
                    .bind(&id)
                    .bind(perms.can_download)
                    .bind(perms.can_manage_favorites)
                    .bind(perms.can_trigger_catalog_sync)
                    .bind(perms.can_force_refresh)
                    .bind(perms.can_view_modules)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
        }

        Ok(User {
            id,
            username: username.to_string(),
            password_hash: pwd_hash,
            role,
            sso_provider: sso_provider.map(String::from),
            sso_subject: sso_subject.map(String::from),
            created_at: now,
            updated_at: now,
            last_login_at: None,
            is_active: true,
        })
    }

    pub async fn get_user_by_id(&self, id: &str) -> Result<Option<User>> {
        let user = db_fetch_optional_map!(
            self,
            "SELECT id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, last_login_at, is_active FROM users WHERE id = ?1",
            [id],
            |r| {
                let role_str: String = r.get("role");
                User {
                    id: r.get("id"),
                    username: r.get("username"),
                    password_hash: r.get("password_hash"),
                    role: Role::from_str(&role_str),
                    sso_provider: r.get("sso_provider"),
                    sso_subject: r.get("sso_subject"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    last_login_at: r.get("last_login_at"),
                    is_active: r.get("is_active"),
                }
            }
        )?;
        Ok(user)
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let user = db_fetch_optional_map!(
            self,
            "SELECT id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, last_login_at, is_active FROM users WHERE username = ?1",
            [username],
            |r| {
                let role_str: String = r.get("role");
                User {
                    id: r.get("id"),
                    username: r.get("username"),
                    password_hash: r.get("password_hash"),
                    role: Role::from_str(&role_str),
                    sso_provider: r.get("sso_provider"),
                    sso_subject: r.get("sso_subject"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    last_login_at: r.get("last_login_at"),
                    is_active: r.get("is_active"),
                }
            }
        )?;
        Ok(user)
    }

    pub async fn list_users_with_permissions(&self) -> Result<Vec<(User, UserPermissions)>> {
        let list = db_fetch_all_map!(
            self,
            r#"
            SELECT
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject,
                u.created_at, u.updated_at, u.last_login_at, u.is_active,
                p.can_download, p.can_manage_favorites, p.can_trigger_catalog_sync, p.can_force_refresh, p.can_view_modules
            FROM users u
            LEFT JOIN user_permissions p ON p.user_id = u.id
            ORDER BY u.created_at ASC
            "#,
            [],
            |r| {
                let role_str: String = r.get("role");
                let role = Role::from_str(&role_str);
                let user = User {
                    id: r.get("id"),
                    username: r.get("username"),
                    password_hash: r.get("password_hash"),
                    role,
                    sso_provider: r.get("sso_provider"),
                    sso_subject: r.get("sso_subject"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    last_login_at: r.get("last_login_at"),
                    is_active: r.get("is_active"),
                };
                let perms = UserPermissions {
                    user_id: user.id.clone(),
                    can_download: r.try_get("can_download").unwrap_or(true),
                    can_manage_favorites: r.try_get("can_manage_favorites").unwrap_or(true),
                    can_trigger_catalog_sync: r.try_get("can_trigger_catalog_sync").unwrap_or(true),
                    can_force_refresh: r.try_get("can_force_refresh").unwrap_or(true),
                    can_view_modules: r.try_get("can_view_modules").unwrap_or(true),
                };
                (user, perms)
            }
        )?;

        Ok(list)
    }

    pub async fn get_user_permissions(&self, user_id: &str) -> Result<UserPermissions> {
        let perms = db_fetch_optional_map!(
            self,
            "SELECT can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules FROM user_permissions WHERE user_id = ?1",
            [user_id],
            |r| UserPermissions {
                user_id: user_id.to_string(),
                can_download: r.get("can_download"),
                can_manage_favorites: r.get("can_manage_favorites"),
                can_trigger_catalog_sync: r.get("can_trigger_catalog_sync"),
                can_force_refresh: r.get("can_force_refresh"),
                can_view_modules: r.get("can_view_modules"),
            }
        )?;

        Ok(perms.unwrap_or_else(|| UserPermissions {
            user_id: user_id.to_string(),
            ..Default::default()
        }))
    }

    pub async fn update_user(
        &self,
        id: &str,
        role: Option<Role>,
        is_active: Option<bool>,
        password: Option<&str>,
    ) -> Result<()> {
        let pwd_hash = match password {
            Some(p) => Some(crate::auth::hash_password(p)?),
            None => None,
        };

        if let Some(r) = role {
            db_execute!(
                self,
                "UPDATE users SET role = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                r.as_str(),
                id,
            )?;
        }

        if let Some(active) = is_active {
            db_execute!(
                self,
                "UPDATE users SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                active,
                id,
            )?;
        }

        if let Some(hash) = pwd_hash {
            db_execute!(
                self,
                "UPDATE users SET password_hash = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2",
                hash,
                id,
            )?;
        }

        Ok(())
    }

    pub async fn update_user_permissions(
        &self,
        user_id: &str,
        permissions: &UserPermissions,
    ) -> Result<()> {
        db_execute!(
            self,
            r#"
            INSERT INTO user_permissions (user_id, can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(user_id) DO UPDATE SET
                can_download = excluded.can_download,
                can_manage_favorites = excluded.can_manage_favorites,
                can_trigger_catalog_sync = excluded.can_trigger_catalog_sync,
                can_force_refresh = excluded.can_force_refresh,
                can_view_modules = excluded.can_view_modules
            "#,
            user_id,
            permissions.can_download,
            permissions.can_manage_favorites,
            permissions.can_trigger_catalog_sync,
            permissions.can_force_refresh,
            permissions.can_view_modules,
        )?;

        Ok(())
    }

    pub async fn update_user_last_login(&self, user_id: &str) -> Result<()> {
        db_execute!(
            self,
            "UPDATE users SET last_login_at = CURRENT_TIMESTAMP WHERE id = ?1",
            user_id,
        )?;
        Ok(())
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<()> {
        db_execute!(self, "DELETE FROM users WHERE id = ?1", user_id)?;
        Ok(())
    }

    // ==========================================
    // SESSIONS & API TOKENS
    // ==========================================

    pub async fn create_session(
        &self,
        user_id: &str,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
        ttl_days: i64,
    ) -> Result<String> {
        let raw_token = generate_secure_token();
        let token_hash = hash_token(&raw_token);
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::days(ttl_days);

        db_execute!(
            self,
            r#"
            INSERT INTO user_sessions (id, token_hash, user_id, created_at, expires_at, last_active_at, user_agent, ip_address)
            VALUES (?1, ?2, ?3, ?4, ?5, ?4, ?6, ?7)
            "#,
            id,
            token_hash,
            user_id,
            now,
            expires_at,
            user_agent,
            ip_address,
        )?;

        Ok(raw_token)
    }

    pub async fn validate_session_or_token(&self, raw_token: &str) -> Result<Option<AuthContext>> {
        let token_hash = hash_token(raw_token);
        let now = Utc::now();

        // 1. Try finding in user_sessions
        let session_opt = db_fetch_optional_map!(
            self,
            r#"
            SELECT
                s.id as session_id, s.user_id, s.expires_at,
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject,
                u.created_at, u.updated_at, u.last_login_at, u.is_active
            FROM user_sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = ?1
            "#,
            [&token_hash],
            |r| {
                let role_str: String = r.get("role");
                let session_id: String = r.get("session_id");
                let user_id: String = r.get("user_id");
                let expires_at: chrono::DateTime<Utc> = r.get("expires_at");
                let is_active: bool = r.get("is_active");
                let user = User {
                    id: user_id.clone(),
                    username: r.get("username"),
                    password_hash: r.get("password_hash"),
                    role: Role::from_str(&role_str),
                    sso_provider: r.get("sso_provider"),
                    sso_subject: r.get("sso_subject"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    last_login_at: r.get("last_login_at"),
                    is_active: true,
                };
                (session_id, user_id, expires_at, is_active, user)
            }
        )?;

        if let Some((session_id, user_id, expires_at, is_active, user)) = session_opt {
            if is_active && expires_at > now {
                let _ = db_execute!(
                    self,
                    "UPDATE user_sessions SET last_active_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    session_id,
                );

                let permissions = self.get_user_permissions(&user_id).await?;
                return Ok(Some(AuthContext {
                    user,
                    permissions,
                    is_api_token: false,
                }));
            }
        }

        // 2. Try finding in api_tokens
        let token_opt = db_fetch_optional_map!(
            self,
            r#"
            SELECT
                t.id as token_id, t.user_id, t.name, t.expires_at,
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject,
                u.created_at, u.updated_at, u.last_login_at, u.is_active
            FROM api_tokens t
            JOIN users u ON u.id = t.user_id
            WHERE t.token_hash = ?1
            "#,
            [&token_hash],
            |r| {
                let role_str: String = r.get("role");
                let token_id: String = r.get("token_id");
                let user_id: String = r.get("user_id");
                let expires_at: Option<chrono::DateTime<Utc>> = r.get("expires_at");
                let is_active: bool = r.get("is_active");
                let user = User {
                    id: user_id.clone(),
                    username: r.get("username"),
                    password_hash: r.get("password_hash"),
                    role: Role::from_str(&role_str),
                    sso_provider: r.get("sso_provider"),
                    sso_subject: r.get("sso_subject"),
                    created_at: r.get("created_at"),
                    updated_at: r.get("updated_at"),
                    last_login_at: r.get("last_login_at"),
                    is_active: true,
                };
                (token_id, user_id, expires_at, is_active, user)
            }
        )?;

        if let Some((token_id, user_id, expires_at, is_active, user)) = token_opt {
            let is_valid = is_active
                && match expires_at {
                    Some(exp) => exp > now,
                    None => true,
                };

            if is_valid {
                let _ = db_execute!(
                    self,
                    "UPDATE api_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1",
                    token_id,
                );

                let permissions = self.get_user_permissions(&user_id).await?;
                return Ok(Some(AuthContext {
                    user,
                    permissions,
                    is_api_token: true,
                }));
            }
        }

        Ok(None)
    }

    pub async fn delete_session(&self, raw_token: &str) -> Result<()> {
        let token_hash = hash_token(raw_token);
        db_execute!(
            self,
            "DELETE FROM user_sessions WHERE token_hash = ?1",
            token_hash,
        )?;
        Ok(())
    }

    pub async fn create_api_token(
        &self,
        user_id: &str,
        name: &str,
        expires_in_days: Option<i64>,
    ) -> Result<(String, crate::auth::ApiToken)> {
        let raw_token = format!("fmd_{}", generate_secure_token());
        let token_hash = hash_token(&raw_token);
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = expires_in_days.map(|d| now + chrono::Duration::days(d));

        db_execute!(
            self,
            r#"
            INSERT INTO api_tokens (id, token_hash, user_id, name, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            &id,
            &token_hash,
            user_id,
            name,
            now,
            expires_at,
        )?;

        Ok((
            raw_token,
            crate::auth::ApiToken {
                id,
                token_hash,
                user_id: user_id.to_string(),
                name: name.to_string(),
                created_at: now,
                expires_at,
                last_used_at: None,
            },
        ))
    }

    pub async fn list_api_tokens_for_user(
        &self,
        user_id: &str,
    ) -> Result<Vec<crate::auth::ApiToken>> {
        let list = db_fetch_all_map!(
            self,
            "SELECT id, token_hash, user_id, name, created_at, expires_at, last_used_at FROM api_tokens WHERE user_id = ?1 ORDER BY created_at DESC",
            [user_id],
            |r| crate::auth::ApiToken {
                id: r.get("id"),
                token_hash: r.get("token_hash"),
                user_id: r.get("user_id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                expires_at: r.get("expires_at"),
                last_used_at: r.get("last_used_at"),
            }
        )?;
        Ok(list)
    }

    pub async fn delete_api_token(&self, user_id: &str, token_id: &str) -> Result<()> {
        db_execute!(
            self,
            "DELETE FROM api_tokens WHERE id = ?1 AND user_id = ?2",
            token_id,
            user_id,
        )?;
        Ok(())
    }

    // ==========================================
    // PER-MODULE CACHE & POLICIES
    // ==========================================

    pub async fn get_manga_cache(&self, manga_id: &str) -> Result<Option<MangaCacheItem>> {
        let item = db_fetch_optional_map!(
            self,
            r#"
            SELECT manga_id, module_id, title, cover_url, metadata_json, chapters_json, cached_at, expires_at
            FROM manga_cache
            WHERE manga_id = ?1
            "#,
            [manga_id],
            |r| MangaCacheItem {
                manga_id: r.get("manga_id"),
                module_id: r.get("module_id"),
                title: r.get("title"),
                cover_url: r.get("cover_url"),
                metadata_json: r.get("metadata_json"),
                chapters_json: r.get("chapters_json"),
                cached_at: r.get("cached_at"),
                expires_at: r.get("expires_at"),
            }
        )?;
        Ok(item)
    }

    pub async fn save_manga_cache(&self, item: &MangaCacheItem) -> Result<()> {
        db_execute!(
            self,
            r#"
            INSERT INTO manga_cache (manga_id, module_id, title, cover_url, metadata_json, chapters_json, cached_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(manga_id) DO UPDATE SET
                title = excluded.title,
                cover_url = excluded.cover_url,
                metadata_json = excluded.metadata_json,
                chapters_json = excluded.chapters_json,
                cached_at = excluded.cached_at,
                expires_at = excluded.expires_at
            "#,
            &item.manga_id,
            &item.module_id,
            &item.title,
            &item.cover_url,
            &item.metadata_json,
            &item.chapters_json,
            item.cached_at,
            item.expires_at,
        )?;

        Ok(())
    }

    pub async fn delete_manga_cache(&self, manga_id: &str) -> Result<()> {
        db_execute!(
            self,
            "DELETE FROM manga_cache WHERE manga_id = ?1",
            manga_id,
        )?;
        Ok(())
    }

    pub async fn get_module_cache_policy(&self, module_id: &str) -> Result<ModuleCachePolicy> {
        let row = db_fetch_optional_map!(
            self,
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies WHERE module_id = ?1",
            [module_id],
            |r| {
                let unit_str: String = r.get("ttl_unit");
                ModuleCachePolicy {
                    module_id: r.get("module_id"),
                    ttl_value: r.get::<i64, _>("ttl_value") as u32,
                    ttl_unit: TtlUnit::from_str_loose(&unit_str),
                    min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                    updated_at: r.get("updated_at"),
                }
            }
        )?;

        if let Some(policy) = row {
            return Ok(policy);
        }

        // Fallback to global policy "*"
        let fallback_row = db_fetch_optional_map!(
            self,
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies WHERE module_id = '*'",
            [],
            |r| {
                let unit_str: String = r.get("ttl_unit");
                ModuleCachePolicy {
                    module_id: module_id.to_string(),
                    ttl_value: r.get::<i64, _>("ttl_value") as u32,
                    ttl_unit: TtlUnit::from_str_loose(&unit_str),
                    min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                    updated_at: r.get("updated_at"),
                }
            }
        )?;

        if let Some(policy) = fallback_row {
            return Ok(policy);
        }

        Ok(ModuleCachePolicy {
            module_id: module_id.to_string(),
            ..Default::default()
        })
    }

    pub async fn list_module_cache_policies(&self) -> Result<Vec<ModuleCachePolicy>> {
        let list = db_fetch_all_map!(
            self,
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies ORDER BY module_id ASC",
            [],
            |r| {
                let unit_str: String = r.get("ttl_unit");
                ModuleCachePolicy {
                    module_id: r.get("module_id"),
                    ttl_value: r.get::<i64, _>("ttl_value") as u32,
                    ttl_unit: TtlUnit::from_str_loose(&unit_str),
                    min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                    updated_at: r.get("updated_at"),
                }
            }
        )?;
        Ok(list)
    }

    pub async fn set_module_cache_policy(&self, policy: &ModuleCachePolicy) -> Result<()> {
        db_execute!(
            self,
            r#"
            INSERT INTO module_cache_policies (module_id, ttl_value, ttl_unit, min_delay_ms, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(module_id) DO UPDATE SET
                ttl_value = excluded.ttl_value,
                ttl_unit = excluded.ttl_unit,
                min_delay_ms = excluded.min_delay_ms,
                updated_at = CURRENT_TIMESTAMP
            "#,
            &policy.module_id,
            policy.ttl_value as i64,
            policy.ttl_unit.as_str(),
            policy.min_delay_ms as i64,
        )?;

        Ok(())
    }

    // ==========================================
    // DOWNLOADS, FAVORITES & STATS HELPERS
    // ==========================================

    #[allow(clippy::too_many_arguments)]
    pub async fn queue_download(
        &self,
        id: &str,
        manga_title: &str,
        chapter_title: &str,
        chapter_url: &str,
        module_id: &str,
        format: &str,
        max_retries: i64,
    ) -> Result<()> {
        db_execute!(
            self,
            r#"
            INSERT INTO downloads (id, manga_title, chapter_title, chapter_url, module_id, format, status, retry_count, max_retries)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'Pending', 0, ?7)
            "#,
            id,
            manga_title,
            chapter_title,
            chapter_url,
            module_id,
            format,
            max_retries,
        )?;
        Ok(())
    }

    pub async fn fetch_next_pending_download(
        &self,
    ) -> Result<Option<(String, String, String, String, String, String, i64, i64)>> {
        let row = db_query_as_fetch_optional!(
            self,
            (String, String, String, String, String, String, i64, i64),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE status = 'Pending' ORDER BY created_at ASC LIMIT 1"
        )?;
        Ok(row)
    }

    pub async fn set_download_status(&self, id: &str, status: &str) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = ?1 WHERE id = ?2",
            status,
            id,
        )?;
        Ok(())
    }

    pub async fn set_download_total_pages(&self, id: &str, total_pages: i64) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET total_pages = ?1 WHERE id = ?2",
            total_pages,
            id,
        )?;
        Ok(())
    }

    pub async fn set_download_progress(&self, id: &str, downloaded_pages: i64) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET downloaded_pages = ?1 WHERE id = ?2",
            downloaded_pages,
            id,
        )?;
        Ok(())
    }

    pub async fn mark_download_completed(&self, id: &str, total_pages: i64) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = 'Completed', downloaded_pages = ?1 WHERE id = ?2",
            total_pages,
            id,
        )?;
        Ok(())
    }

    pub async fn mark_download_failed(&self, id: &str, error_message: &str) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = 'Failed', error_message = ?1 WHERE id = ?2",
            error_message,
            id,
        )?;
        Ok(())
    }

    pub async fn schedule_download_retry(
        &self,
        id: &str,
        next_retry: i64,
        retry_note: &str,
    ) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = 'Pending', retry_count = ?1, error_message = ?2, created_at = CURRENT_TIMESTAMP WHERE id = ?3",
            next_retry,
            retry_note,
            id,
        )?;
        Ok(())
    }

    pub async fn get_manga_metadata_by_title(
        &self,
        title: &str,
        module_id: &str,
    ) -> Result<
        Option<(
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        let row = db_query_as_fetch_optional!(
            self,
            (Option<String>, Option<String>, Option<String>, Option<String>),
            "SELECT author, description, genres, original_url FROM mangas WHERE title = ?1 AND module_id = ?2 LIMIT 1",
            title,
            module_id,
        )?;
        Ok(row)
    }

    pub async fn list_downloads_summary(
        &self,
        limit: i64,
    ) -> Result<
        Vec<(
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
        )>,
    > {
        let rows = db_query_as_fetch_all!(
            self,
            (String, String, String, String, String, String, i64, i64, String, Option<String>, i64, i64),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, total_pages, downloaded_pages, status, error_message, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads ORDER BY created_at DESC LIMIT ?1",
            limit,
        )?;
        Ok(rows)
    }

    pub async fn delete_download(&self, id: &str) -> Result<()> {
        db_execute!(self, "DELETE FROM downloads WHERE id = ?1", id)?;
        Ok(())
    }

    pub async fn get_download_retry_info(
        &self,
        id: &str,
    ) -> Result<Option<(String, String, String, String, String, String, i64, i64)>> {
        let row = db_query_as_fetch_optional!(
            self,
            (String, String, String, String, String, String, i64, i64),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE id = ?1",
            id,
        )?;
        Ok(row)
    }

    pub async fn reset_download_for_retry(&self, id: &str) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = 'Pending', error_message = NULL, downloaded_pages = 0, retry_count = 0, created_at = CURRENT_TIMESTAMP WHERE id = ?1",
            id,
        )?;
        Ok(())
    }

    pub async fn list_failed_downloads(
        &self,
    ) -> Result<Vec<(String, String, String, String, String, String, i64, i64)>> {
        let rows = db_query_as_fetch_all!(
            self,
            (String, String, String, String, String, String, i64, i64),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, CAST(0 AS BIGINT), COALESCE(max_retries, 3) FROM downloads WHERE status = 'Failed'",
        )?;
        Ok(rows)
    }

    pub async fn reset_all_failed_downloads(&self) -> Result<u64> {
        let affected = db_execute!(
            self,
            "UPDATE downloads SET status = 'Pending', error_message = NULL, downloaded_pages = 0, retry_count = 0, created_at = CURRENT_TIMESTAMP WHERE status = 'Failed'",
        )?;
        Ok(affected)
    }

    pub async fn get_download_chapter_url(&self, id: &str) -> Result<Option<String>> {
        let url = db_query_scalar_fetch_optional!(
            self,
            String,
            "SELECT chapter_url FROM downloads WHERE id = ?1",
            id,
        )?;
        Ok(url)
    }

    pub async fn list_incomplete_downloads(
        &self,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let rows = db_query_as_fetch_all!(
            self,
            (String, String, String, String, String),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id FROM downloads WHERE status != 'Completed' ORDER BY created_at DESC",
        )?;
        Ok(rows)
    }

    pub async fn delete_favorite(&self, manga_id: &str) -> Result<()> {
        db_execute!(self, "DELETE FROM favorites WHERE manga_id = ?1", manga_id,)?;
        Ok(())
    }

    pub async fn count_mangas(&self) -> Result<i64> {
        let row: (i64,) = db_query_as_fetch_one!(self, (i64,), "SELECT COUNT(*) FROM mangas")?;
        Ok(row.0)
    }

    pub async fn count_chapters(&self) -> Result<i64> {
        let row: (i64,) = db_query_as_fetch_one!(self, (i64,), "SELECT COUNT(*) FROM chapters")?;
        Ok(row.0)
    }

    pub async fn get_first_manga_meta(
        &self,
    ) -> Result<(
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let row = db_query_as_fetch_one!(
            self,
            (
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>
            ),
            "SELECT cover_url, author, description, genres FROM mangas LIMIT 1"
        )?;
        Ok(row)
    }

    pub async fn list_mangas_summary(
        &self,
        limit: i64,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let rows = db_query_as_fetch_all!(
            self,
            (String, String, String, String, String),
            "SELECT id, module_id, title, original_url, status FROM mangas LIMIT ?1",
            limit,
        )?;
        Ok(rows)
    }

    pub async fn get_manga_metadata_for_packaging(
        &self,
        manga_title: &str,
        module_id: &str,
    ) -> Result<
        Option<(
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )>,
    > {
        let prefix_id = format!("{}:%", module_id);
        let like_op = if self.is_postgres() { "ILIKE" } else { "LIKE" };
        let sql = format!(
            "SELECT author, genres, description, original_url FROM mangas WHERE title = ?1 OR id {like_op} ?2 ORDER BY (author IS NOT NULL AND author != '') DESC, updated_at DESC LIMIT 1"
        );
        let row = db_query_as_fetch_optional!(
            self,
            (
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>
            ),
            &sql,
            manga_title,
            &prefix_id,
        )?;
        Ok(row)
    }

    #[allow(clippy::type_complexity)]
    pub async fn list_downloads_with_created_at(
        &self,
        limit: i64,
    ) -> Result<
        Vec<(
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
        )>,
    > {
        let rows = db_fetch_all_map!(
            self,
            r#"
            SELECT id, manga_title, chapter_title, chapter_url, module_id, format,
                   total_pages, downloaded_pages, status, error_message,
                   COALESCE(retry_count, 0) as retry_count, COALESCE(max_retries, 3) as max_retries, created_at
            FROM downloads
            ORDER BY created_at DESC
            LIMIT ?1
            "#,
            [limit],
            |r| {
                let created_at: chrono::DateTime<Utc> = r.get("created_at");
                (
                    r.get("id"),
                    r.get("manga_title"),
                    r.get("chapter_title"),
                    r.get("chapter_url"),
                    r.get("module_id"),
                    r.get("format"),
                    r.get("total_pages"),
                    r.get("downloaded_pages"),
                    r.get("status"),
                    r.get("error_message"),
                    r.get("retry_count"),
                    r.get("max_retries"),
                    created_at.to_rfc3339(),
                )
            }
        )?;
        Ok(rows)
    }

    pub async fn list_pending_downloads(
        &self,
    ) -> Result<Vec<(String, String, String, String, String, String, i64, i64)>> {
        let rows = db_query_as_fetch_all!(
            self,
            (String, String, String, String, String, String, i64, i64),
            "SELECT id, manga_title, chapter_title, chapter_url, module_id, format, COALESCE(retry_count, 0), COALESCE(max_retries, 3) FROM downloads WHERE status = 'Pending'",
        )?;
        Ok(rows)
    }

    pub async fn update_download_progress_full(
        &self,
        id: &str,
        status: &str,
        downloaded_pages: i64,
        total_pages: Option<i64>,
        error_message: Option<&str>,
    ) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = ?1, downloaded_pages = ?2, total_pages = COALESCE(?3, total_pages), error_message = ?4 WHERE id = ?5",
            status,
            downloaded_pages,
            total_pages,
            error_message,
            id,
        )?;
        Ok(())
    }

    pub async fn mark_download_completed_full(&self, id: &str) -> Result<()> {
        db_execute!(
            self,
            "UPDATE downloads SET status = 'Completed', downloaded_pages = total_pages, error_message = NULL WHERE id = ?1",
            id,
        )?;
        Ok(())
    }

    pub async fn get_download_status_and_retry(
        &self,
        id: &str,
    ) -> Result<Option<(String, i64, Option<String>)>> {
        let row = db_query_as_fetch_optional!(
            self,
            (String, i64, Option<String>),
            "SELECT status, retry_count, error_message FROM downloads WHERE id = ?1",
            id,
        )?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_init() {
        let db = Database::new(":memory:").await.unwrap();
        // Check that tables exist
        assert_eq!(db.count_mangas().await.unwrap(), 0);

        // Test catalog upsert and search
        let items = vec![
            (
                "Jujutsu Kaisen".to_string(),
                "novel/Jujutsu-Kaisen.html".to_string(),
            ),
            ("One Piece".to_string(), "novel/One-Piece.html".to_string()),
            (
                "Chainsaw Man".to_string(),
                "novel/Chainsaw-Man.html".to_string(),
            ),
        ];
        let count = db
            .upsert_catalog_items("NovelCoolFR", &items)
            .await
            .unwrap();
        assert_eq!(count, 3);

        let (search_res, total) = db.search_catalog("jujutsu", None, 10, 0).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(search_res[0].title, "Jujutsu Kaisen");

        // Test module sync status
        db.set_module_sync_status("NovelCoolFR", "Completed", None, Some(3))
            .await
            .unwrap();
        let status = db
            .get_module_sync_status("NovelCoolFR")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(status.status, "Completed");
        assert_eq!(status.total_mangas, 3);

        // Test favorite creation and chapter tracking (Lot 3)
        db.add_or_update_favorite(
            "m1",
            "NovelCoolFR",
            "Jujutsu Kaisen",
            "https://fr.novelcool.com/novel/Jujutsu-Kaisen.html",
            Some("https://wpimg.cc/cover.jpg"),
            true,
            "CBZ",
        )
        .await
        .unwrap();

        let initial_chapters = vec![
            crate::models::ChapterItem {
                title: "Ch 1".into(),
                url: "ch1".into(),
            },
            crate::models::ChapterItem {
                title: "Ch 2".into(),
                url: "ch2".into(),
            },
        ];
        // Record initial chapters marked as already seen/downloaded
        db.record_chapters("m1", &initial_chapters, true)
            .await
            .unwrap();

        let favs = db.list_favorites_with_stats().await.unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].total_chapters, 2);
        assert_eq!(favs[0].downloaded_chapters_count, 2);
        assert!(favs[0].auto_download);

        // Simulate chapter update: Ch 3 is released!
        let updated_chapters = vec![
            crate::models::ChapterItem {
                title: "Ch 1".into(),
                url: "ch1".into(),
            },
            crate::models::ChapterItem {
                title: "Ch 2".into(),
                url: "ch2".into(),
            },
            crate::models::ChapterItem {
                title: "Ch 3".into(),
                url: "ch3".into(),
            },
        ];
        let new_chaps = db
            .check_favorite_new_chapters("m1", &updated_chapters)
            .await
            .unwrap();
        assert_eq!(new_chaps.len(), 1);
        assert_eq!(new_chaps[0].title, "Ch 3");

        let favs_after = db.list_favorites_with_stats().await.unwrap();
        assert_eq!(favs_after[0].total_chapters, 3);
        assert_eq!(favs_after[0].downloaded_chapters_count, 2); // Ch 3 is not yet downloaded

        // Mark Ch 3 as downloaded
        db.mark_chapter_downloaded("m1", "ch3").await.unwrap();
        let favs_final = db.list_favorites_with_stats().await.unwrap();
        assert_eq!(favs_final[0].downloaded_chapters_count, 3);
    }

    #[tokio::test]
    async fn test_auth_and_cache_features() {
        let db = Database::new(":memory:").await.unwrap();

        // 1. Initially 0 users
        assert_eq!(db.count_users().await.unwrap(), 0);

        // Pre-create an unassigned favorite
        db.add_or_update_favorite(
            "m_legacy",
            "MangaDex",
            "Bleach",
            "https://mangadex.org/title/bleach",
            None,
            true,
            "CBZ",
        )
        .await
        .unwrap();

        // 2. Create initial admin
        let admin = db
            .create_initial_admin("superadmin", "P@ssw0rd123!")
            .await
            .unwrap();
        assert_eq!(admin.username, "superadmin");
        assert_eq!(admin.role, Role::Admin);
        assert_eq!(db.count_users().await.unwrap(), 1);

        // The legacy favorite should now be owned by this admin
        let admin_favs = db.list_favorites_for_user(Some(&admin.id)).await.unwrap();
        assert_eq!(admin_favs.len(), 1);
        assert_eq!(admin_favs[0].title, "Bleach");

        // 3. Create regular user
        let user = db
            .create_user(
                "reader1",
                Some("UserSecret1!"),
                Role::User,
                Some(UserPermissions {
                    user_id: String::new(),
                    can_download: true,
                    can_manage_favorites: true,
                    can_trigger_catalog_sync: false,
                    can_force_refresh: false,
                    can_view_modules: true,
                }),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(db.count_users().await.unwrap(), 2);

        // Reader shouldn't have bleach if user-filtered (unless legacy unassigned)
        let perms = db.get_user_permissions(&user.id).await.unwrap();
        assert!(!perms.can_trigger_catalog_sync);

        // 4. Session authentication
        let session_token = db
            .create_session(&user.id, Some("Firefox/130"), Some("127.0.0.1"), 7)
            .await
            .unwrap();
        let auth_ctx = db
            .validate_session_or_token(&session_token)
            .await
            .unwrap()
            .expect("session should be valid");
        assert_eq!(auth_ctx.user.id, user.id);
        assert!(auth_ctx.permissions.can_download);
        assert!(!auth_ctx.is_api_token);

        // Delete session
        db.delete_session(&session_token).await.unwrap();
        assert!(db
            .validate_session_or_token(&session_token)
            .await
            .unwrap()
            .is_none());

        // 5. API Token authentication
        let (raw_api_token, _token_meta) = db
            .create_api_token(&admin.id, "ci-token", Some(30))
            .await
            .unwrap();
        let api_ctx = db
            .validate_session_or_token(&raw_api_token)
            .await
            .unwrap()
            .expect("api token should be valid");
        assert_eq!(api_ctx.user.id, admin.id);
        assert!(api_ctx.is_api_token);

        // 6. Cache policies
        let default_policy = db.get_module_cache_policy("mangadex").await.unwrap();
        assert_eq!(default_policy.ttl_unit, TtlUnit::Hours);

        let custom_policy = ModuleCachePolicy {
            module_id: "mangadex".to_string(),
            ttl_value: 3,
            ttl_unit: TtlUnit::Days,
            min_delay_ms: 2500,
            updated_at: Utc::now(),
        };
        db.set_module_cache_policy(&custom_policy).await.unwrap();

        let fetched_policy = db.get_module_cache_policy("mangadex").await.unwrap();
        assert_eq!(fetched_policy.ttl_value, 3);
        assert_eq!(fetched_policy.ttl_unit, TtlUnit::Days);
        assert_eq!(fetched_policy.min_delay_ms, 2500);

        // 7. Manga cache item
        let cache_item = MangaCacheItem {
            manga_id: "mangadex:12345".to_string(),
            module_id: "mangadex".to_string(),
            title: "Chainsaw Man".to_string(),
            cover_url: Some("https://example.com/cover.jpg".to_string()),
            metadata_json: r#"{"status":"ongoing"}"#.to_string(),
            chapters_json: r#"[{"title":"Ch 1","url":"/ch1"}]"#.to_string(),
            cached_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::hours(2),
        };
        db.save_manga_cache(&cache_item).await.unwrap();

        let cached = db
            .get_manga_cache("mangadex:12345")
            .await
            .unwrap()
            .expect("cache should exist");
        assert_eq!(cached.title, "Chainsaw Man");
        assert!(!cached.is_expired());
    }

    #[test]
    fn test_to_pg_sql_and_url_detection() {
        assert_eq!(
            to_pg_sql("SELECT * FROM users WHERE id = ?1 AND name = '?2' AND role = ?2"),
            "SELECT * FROM users WHERE id = $1 AND name = '?2' AND role = $2"
        );
        assert!(Database::is_postgres_url(
            "postgres://user:pass@localhost/fmd"
        ));
        assert!(Database::is_postgres_url("POSTGRESQL://localhost:5432/fmd"));
        assert!(!Database::is_postgres_url("sqlite://data/fmd.db"));
        assert_eq!(
            Database::resolve_url(Some("postgres://pg/fmd"), "data/fmd.db"),
            "postgres://pg/fmd"
        );
    }
}
