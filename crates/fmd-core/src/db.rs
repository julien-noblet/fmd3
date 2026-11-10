use crate::auth::{generate_secure_token, hash_token, AuthContext, Role, User, UserPermissions};
use crate::cache::{MangaCacheItem, ModuleCachePolicy, TtlUnit};
use crate::errors::{Error, Result};
use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Row, Sqlite};
use std::path::Path;
use tracing::info;

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    pub async fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let db_str = db_path.as_ref().to_str().ok_or_else(|| {
            Error::Custom("Invalid database path".into())
        })?;

        if let Some(parent) = db_path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let connection_string = format!("sqlite://{}?mode=rwc", db_str);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_string)
            .await?;

        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<()> {
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
        .execute(&self.pool)
        .await?;

        let _ = sqlx::query("ALTER TABLE favorites ADD COLUMN auto_download BOOLEAN NOT NULL DEFAULT 0").execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE favorites ADD COLUMN download_format TEXT NOT NULL DEFAULT 'CBZ'").execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE favorites ADD COLUMN user_id TEXT DEFAULT NULL").execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_favorites_user ON favorites(user_id)").execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE downloads ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0").execute(&self.pool).await;
        let _ = sqlx::query("ALTER TABLE downloads ADD COLUMN max_retries INTEGER NOT NULL DEFAULT 3").execute(&self.pool).await;
        let _ = sqlx::query("CREATE INDEX IF NOT EXISTS idx_chapters_manga_url ON chapters(manga_id, url)").execute(&self.pool).await;
        let _ = sqlx::query("INSERT OR IGNORE INTO module_cache_policies (module_id, ttl_value, ttl_unit, min_delay_ms) VALUES ('*', 24, 'hours', 1000)").execute(&self.pool).await;

        Ok(())
    }

    /// Import legacy FMD2 userdata SQLite database
    pub async fn import_legacy_db<P: AsRef<Path>>(&self, legacy_path: P) -> Result<usize> {
        let path_str = legacy_path.as_ref().to_str().ok_or_else(|| {
            Error::Custom("Invalid legacy path".into())
        })?;

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
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO mangas (id, module_id, title, original_url, status)
                    VALUES (?1, ?2, ?3, ?4, 'ongoing')
                    "#,
                )
                .bind(&manga_id)
                .bind(&module_id)
                .bind(&title)
                .bind(&link)
                .execute(&self.pool)
                .await?;

                // Insert into favorites
                sqlx::query(
                    r#"
                    INSERT OR REPLACE INTO favorites (manga_id, order_index, enabled, current_chapter, save_to)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                )
                .bind(&manga_id)
                .bind(order_index)
                .bind(enabled)
                .bind(current_chapter)
                .bind(save_to)
                .execute(&self.pool)
                .await?;

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
                        let _ = sqlx::query(
                            r#"
                            INSERT OR IGNORE INTO chapters (id, manga_id, title, chapter_number, url, is_downloaded)
                            VALUES (?1, ?2, ?3, 0.0, '', 1)
                            "#,
                        )
                        .bind(&ch_id)
                        .bind(&id)
                        .bind(ch_title)
                        .execute(&self.pool)
                        .await;
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
        let mut tx = self.pool.begin().await?;
        let mut inserted = 0;
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
                sqlx::query_as("SELECT COUNT(*) FROM catalog WHERE module_id = ?1")
                    .bind(m_id)
                    .fetch_one(&self.pool)
                    .await?
            } else {
                sqlx::query_as("SELECT COUNT(*) FROM catalog")
                    .fetch_one(&self.pool)
                    .await?
            };

            let rows = if let Some(m_id) = module_id {
                sqlx::query(
                    r#"
                    SELECT id, module_id, title, link, cover_url, updated_at
                    FROM catalog
                    WHERE module_id = ?1
                    ORDER BY title ASC
                    LIMIT ?2 OFFSET ?3
                    "#,
                )
                .bind(m_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query(
                    r#"
                    SELECT id, module_id, title, link, cover_url, updated_at
                    FROM catalog
                    ORDER BY title ASC
                    LIMIT ?1 OFFSET ?2
                    "#,
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(&self.pool)
                .await?
            };

            let items = rows.into_iter().map(|r| {
                crate::models::CatalogItem {
                    id: r.get("id"),
                    module_id: r.get("module_id"),
                    title: r.get("title"),
                    link: r.get("link"),
                    cover_url: r.get("cover_url"),
                    updated_at: r.get("updated_at"),
                }
            }).collect();

            return Ok((items, total_row.0));
        }

        // FTS5 search with fallback to LIKE
        let fts_query = q.split_whitespace()
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
                .fetch_all(&self.pool)
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
                .fetch_all(&self.pool)
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
                .fetch_one(&self.pool)
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
                .fetch_one(&self.pool)
                .await
                .unwrap_or((rows.len() as i64,))
            };

            let items = rows.into_iter().map(|r| {
                crate::models::CatalogItem {
                    id: r.get("id"),
                    module_id: r.get("module_id"),
                    title: r.get("title"),
                    link: r.get("link"),
                    cover_url: r.get("cover_url"),
                    updated_at: r.get("updated_at"),
                }
            }).collect();

            return Ok((items, total_count.0));
        }

        // Substring LIKE fallback
        let like_pat = format!("%{}%", q);
        let total_row: (i64,) = if let Some(m_id) = module_id {
            sqlx::query_as("SELECT COUNT(*) FROM catalog WHERE module_id = ?1 AND title LIKE ?2")
                .bind(m_id)
                .bind(&like_pat)
                .fetch_one(&self.pool)
                .await?
        } else {
            sqlx::query_as("SELECT COUNT(*) FROM catalog WHERE title LIKE ?1")
                .bind(&like_pat)
                .fetch_one(&self.pool)
                .await?
        };

        let rows = if let Some(m_id) = module_id {
            sqlx::query(
                r#"
                SELECT id, module_id, title, link, cover_url, updated_at
                FROM catalog
                WHERE module_id = ?1 AND title LIKE ?2
                ORDER BY title ASC
                LIMIT ?3 OFFSET ?4
                "#,
            )
            .bind(m_id)
            .bind(&like_pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, module_id, title, link, cover_url, updated_at
                FROM catalog
                WHERE title LIKE ?1
                ORDER BY title ASC
                LIMIT ?2 OFFSET ?3
                "#,
            )
            .bind(&like_pat)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        let items = rows.into_iter().map(|r| {
            crate::models::CatalogItem {
                id: r.get("id"),
                module_id: r.get("module_id"),
                title: r.get("title"),
                link: r.get("link"),
                cover_url: r.get("cover_url"),
                updated_at: r.get("updated_at"),
            }
        }).collect();

        Ok((items, total_row.0))
    }

    pub async fn get_module_sync_status(&self, module_id: &str) -> Result<Option<crate::models::ModuleSyncStatus>> {
        let row_opt = sqlx::query(
            "SELECT module_id, total_mangas, status, last_synced_at, error_message FROM module_sync_status WHERE module_id = ?1"
        )
        .bind(module_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row_opt {
            Ok(Some(crate::models::ModuleSyncStatus {
                module_id: r.get("module_id"),
                total_mangas: r.get("total_mangas"),
                status: r.get("status"),
                last_synced_at: r.get("last_synced_at"),
                error_message: r.get("error_message"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn set_module_sync_status(
        &self,
        module_id: &str,
        status: &str,
        error_message: Option<&str>,
        total_mangas: Option<i64>,
    ) -> Result<()> {
        let total = total_mangas.unwrap_or(0);
        sqlx::query(
            r#"
            INSERT INTO module_sync_status (module_id, total_mangas, status, last_synced_at, error_message)
            VALUES (?1, ?2, ?3, CURRENT_TIMESTAMP, ?4)
            ON CONFLICT(module_id) DO UPDATE SET
                status = excluded.status,
                last_synced_at = CURRENT_TIMESTAMP,
                error_message = excluded.error_message,
                total_mangas = CASE WHEN excluded.total_mangas > 0 THEN excluded.total_mangas ELSE module_sync_status.total_mangas END
            "#
        )
        .bind(module_id)
        .bind(total)
        .bind(status)
        .bind(error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_all_sync_statuses(&self) -> Result<Vec<crate::models::ModuleSyncStatus>> {
        let rows = sqlx::query(
            "SELECT module_id, total_mangas, status, last_synced_at, error_message FROM module_sync_status"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut list = Vec::new();
        for r in rows {
            list.push(crate::models::ModuleSyncStatus {
                module_id: r.get("module_id"),
                total_mangas: r.get("total_mangas"),
                status: r.get("status"),
                last_synced_at: r.get("last_synced_at"),
                error_message: r.get("error_message"),
            });
        }
        Ok(list)
    }

    pub async fn get_catalog_total_count(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM catalog")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

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
        sqlx::query(
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
        )
        .bind(manga_id)
        .bind(module_id)
        .bind(title)
        .bind(original_url)
        .bind(cover_url)
        .bind(author)
        .bind(description)
        .bind(genres_json)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

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
        let mut tx = self.pool.begin().await?;
        let genres_json = genres.map(|g| serde_json::to_string(g).unwrap_or_default());

        sqlx::query(
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
        )
        .bind(manga_id)
        .bind(module_id)
        .bind(title)
        .bind(original_url)
        .bind(cover_url)
        .bind(author)
        .bind(description)
        .bind(genres_json)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO favorites (manga_id, user_id, enabled, auto_download, download_format, last_checked_at, last_updated_at)
            VALUES (?1, ?2, 1, ?3, ?4, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
            ON CONFLICT(manga_id) DO UPDATE SET
                user_id = COALESCE(favorites.user_id, excluded.user_id),
                auto_download = excluded.auto_download,
                download_format = excluded.download_format
            "#,
        )
        .bind(manga_id)
        .bind(user_id)
        .bind(auto_download)
        .bind(download_format)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn record_chapters(
        &self,
        manga_id: &str,
        chapters: &[crate::models::ChapterItem],
        mark_as_downloaded: bool,
    ) -> Result<usize> {
        let mut tx = self.pool.begin().await?;
        let mut count = 0;
        for ch in chapters {
            let ch_id = format!("{}:{}", manga_id, ch.url);
            let res = sqlx::query(
                r#"
                INSERT INTO chapters (id, manga_id, title, url, is_downloaded, downloaded_at)
                VALUES (?1, ?2, ?3, ?4, ?5, CASE WHEN ?5 = 1 THEN CURRENT_TIMESTAMP ELSE NULL END)
                ON CONFLICT(id) DO UPDATE SET
                    title = excluded.title,
                    is_downloaded = CASE WHEN excluded.is_downloaded = 1 THEN 1 ELSE chapters.is_downloaded END
                "#,
            )
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
        Ok(count)
    }

    pub async fn check_favorite_new_chapters(
        &self,
        manga_id: &str,
        fetched_chapters: &[crate::models::ChapterItem],
    ) -> Result<Vec<crate::models::ChapterItem>> {
        let rows = sqlx::query("SELECT url FROM chapters WHERE manga_id = ?1")
            .bind(manga_id)
            .fetch_all(&self.pool)
            .await?;

        let known_urls: std::collections::HashSet<String> = rows
            .into_iter()
            .map(|r| r.get::<String, _>("url"))
            .collect();

        let mut new_chapters = Vec::new();
        for ch in fetched_chapters {
            if !known_urls.contains(&ch.url) {
                new_chapters.push(ch.clone());
            }
        }

        if !new_chapters.is_empty() {
            self.record_chapters(manga_id, &new_chapters, false).await?;
            sqlx::query("UPDATE favorites SET last_checked_at = CURRENT_TIMESTAMP, last_updated_at = CURRENT_TIMESTAMP WHERE manga_id = ?1")
                .bind(manga_id)
                .execute(&self.pool)
                .await?;
        } else {
            sqlx::query("UPDATE favorites SET last_checked_at = CURRENT_TIMESTAMP WHERE manga_id = ?1")
                .bind(manga_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(new_chapters)
    }

    pub async fn mark_chapter_downloaded(&self, manga_id: &str, chapter_url: &str) -> Result<()> {
        let ch_id = format!("{}:{}", manga_id, chapter_url);
        sqlx::query(
            r#"
            UPDATE chapters 
            SET is_downloaded = 1, downloaded_at = CURRENT_TIMESTAMP 
            WHERE (id = ?1 AND ?1 != '') OR (manga_id = ?2 AND url = ?3) OR url = ?3
            "#,
        )
        .bind(&ch_id)
        .bind(manga_id)
        .bind(chapter_url)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn reconcile_downloaded_chapters(&self) -> Result<usize> {
        let res = sqlx::query(
            r#"
            UPDATE chapters 
            SET is_downloaded = 0, downloaded_at = NULL 
            WHERE is_downloaded = 1 
              AND url NOT IN (SELECT chapter_url FROM downloads WHERE status = 'Completed')
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(res.rows_affected() as usize)
    }

    pub async fn list_favorites_with_stats(&self) -> Result<Vec<crate::models::Favorite>> {
        self.list_favorites_for_user(None).await
    }

    pub async fn list_favorites_for_user(&self, user_id: Option<&str>) -> Result<Vec<crate::models::Favorite>> {
        let rows = match user_id {
            Some(uid) => {
                sqlx::query(
                    r#"
                    SELECT 
                        f.manga_id, 
                        m.title, 
                        m.module_id,
                        m.original_url, 
                        m.cover_url, 
                        f.order_index, 
                        f.enabled, 
                        COALESCE(f.auto_download, 0) as auto_download,
                        COALESCE(f.download_format, 'CBZ') as download_format,
                        f.current_chapter, 
                        f.save_to,
                        f.last_checked_at,
                        f.last_updated_at,
                        COUNT(c.id) as total_chapters,
                        COALESCE(SUM(CASE WHEN c.is_downloaded = 1 THEN 1 ELSE 0 END), 0) as downloaded_count
                    FROM favorites f
                    JOIN mangas m ON m.id = f.manga_id
                    LEFT JOIN chapters c ON c.manga_id = f.manga_id
                    WHERE f.user_id = ?1 OR f.user_id IS NULL
                    GROUP BY f.manga_id
                    ORDER BY f.order_index, f.last_updated_at DESC
                    "#,
                )
                .bind(uid)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    r#"
                    SELECT 
                        f.manga_id, 
                        m.title, 
                        m.module_id,
                        m.original_url, 
                        m.cover_url, 
                        f.order_index, 
                        f.enabled, 
                        COALESCE(f.auto_download, 0) as auto_download,
                        COALESCE(f.download_format, 'CBZ') as download_format,
                        f.current_chapter, 
                        f.save_to,
                        f.last_checked_at,
                        f.last_updated_at,
                        COUNT(c.id) as total_chapters,
                        COALESCE(SUM(CASE WHEN c.is_downloaded = 1 THEN 1 ELSE 0 END), 0) as downloaded_count
                    FROM favorites f
                    JOIN mangas m ON m.id = f.manga_id
                    LEFT JOIN chapters c ON c.manga_id = f.manga_id
                    GROUP BY f.manga_id
                    ORDER BY f.order_index, f.last_updated_at DESC
                    "#,
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        let mut favs = Vec::new();
        for r in rows {
            favs.push(crate::models::Favorite {
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
            });
        }

        Ok(favs)
    }

    pub async fn update_favorite_settings(
        &self,
        manga_id: &str,
        enabled: bool,
        auto_download: bool,
        download_format: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE favorites SET enabled = ?1, auto_download = ?2, download_format = ?3 WHERE manga_id = ?4"
        )
        .bind(enabled)
        .bind(auto_download)
        .bind(download_format)
        .bind(manga_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_manga_chapters(&self, manga_id: &str) -> Result<Vec<(String, String, bool)>> {
        let rows = sqlx::query(
            "SELECT title, url, is_downloaded FROM chapters WHERE manga_id = ?1 ORDER BY rowid ASC"
        )
        .bind(manga_id)
        .fetch_all(&self.pool)
        .await?;

        let mut list = Vec::new();
        for r in rows {
            list.push((r.get("title"), r.get("url"), r.get("is_downloaded")));
        }
        Ok(list)
    }

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value"
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_settings(&self) -> Result<crate::models::AppSettings> {
        let mut s = crate::models::AppSettings::default();
        let rows = sqlx::query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;

        for r in rows {
            let k: String = r.get("key");
            let v: String = r.get("value");
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
        self.set_setting("download_dir", &settings.download_dir).await?;
        self.set_setting("default_format", &settings.default_format).await?;
        self.set_setting("naming_template", &settings.naming_template).await?;
        self.set_setting("flaresolverr_url", &settings.flaresolverr_url).await?;
        self.set_setting("check_interval_hours", &settings.check_interval_hours.to_string()).await?;
        self.set_setting("catalog_resync_days", &settings.catalog_resync_days.to_string()).await?;
        Ok(())
    }

    // ==========================================
    // USER MANAGEMENT & AUTHENTICATION
    // ==========================================

    pub async fn count_users(&self) -> Result<i64> {
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    pub async fn create_initial_admin(&self, username: &str, password: &str) -> Result<User> {
        let mut tx = self.pool.begin().await?;

        let id = uuid::Uuid::new_v4().to_string();
        let pwd_hash = crate::auth::hash_password(password)?;
        let now = Utc::now();

        sqlx::query(
            r#"
            INSERT INTO users (id, username, password_hash, role, created_at, updated_at, is_active)
            VALUES (?1, ?2, ?3, 'admin', ?4, ?4, 1)
            "#,
        )
        .bind(&id)
        .bind(username)
        .bind(&pwd_hash)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO user_permissions (user_id, can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules)
            VALUES (?1, 1, 1, 1, 1, 1)
            "#,
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;

        // Assign any pre-existing unassigned favorites to this initial administrator
        sqlx::query(
            r#"
            UPDATE favorites SET user_id = ?1 WHERE user_id IS NULL
            "#,
        )
        .bind(&id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

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
        let mut tx = self.pool.begin().await?;
        let id = uuid::Uuid::new_v4().to_string();
        let pwd_hash = match password {
            Some(p) => Some(crate::auth::hash_password(p)?),
            None => None,
        };
        let now = Utc::now();
        let role_str = role.as_str();

        sqlx::query(
            r#"
            INSERT INTO users (id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, is_active)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, 1)
            "#,
        )
        .bind(&id)
        .bind(username)
        .bind(&pwd_hash)
        .bind(role_str)
        .bind(sso_provider)
        .bind(sso_subject)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let perms = permissions.unwrap_or_else(|| {
            if role == Role::Admin {
                UserPermissions::admin_defaults(&id)
            } else {
                UserPermissions::user_defaults(&id)
            }
        });

        sqlx::query(
            r#"
            INSERT INTO user_permissions (user_id, can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&id)
        .bind(perms.can_download)
        .bind(perms.can_manage_favorites)
        .bind(perms.can_trigger_catalog_sync)
        .bind(perms.can_force_refresh)
        .bind(perms.can_view_modules)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

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
        let row = sqlx::query(
            "SELECT id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, last_login_at, is_active FROM users WHERE id = ?1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
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
        }))
    }

    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        let row = sqlx::query(
            "SELECT id, username, password_hash, role, sso_provider, sso_subject, created_at, updated_at, last_login_at, is_active FROM users WHERE username = ?1"
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
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
        }))
    }

    pub async fn list_users_with_permissions(&self) -> Result<Vec<(User, UserPermissions)>> {
        let rows = sqlx::query(
            r#"
            SELECT 
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject, 
                u.created_at, u.updated_at, u.last_login_at, u.is_active,
                p.can_download, p.can_manage_favorites, p.can_trigger_catalog_sync, p.can_force_refresh, p.can_view_modules
            FROM users u
            LEFT JOIN user_permissions p ON p.user_id = u.id
            ORDER BY u.created_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut list = Vec::new();
        for r in rows {
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
            list.push((user, perms));
        }

        Ok(list)
    }

    pub async fn get_user_permissions(&self, user_id: &str) -> Result<UserPermissions> {
        let row = sqlx::query(
            "SELECT can_download, can_manage_favorites, can_trigger_catalog_sync, can_force_refresh, can_view_modules FROM user_permissions WHERE user_id = ?1"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => Ok(UserPermissions {
                user_id: user_id.to_string(),
                can_download: r.get("can_download"),
                can_manage_favorites: r.get("can_manage_favorites"),
                can_trigger_catalog_sync: r.get("can_trigger_catalog_sync"),
                can_force_refresh: r.get("can_force_refresh"),
                can_view_modules: r.get("can_view_modules"),
            }),
            None => Ok(UserPermissions {
                user_id: user_id.to_string(),
                ..Default::default()
            }),
        }
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

        let mut tx = self.pool.begin().await?;

        if let Some(r) = role {
            sqlx::query("UPDATE users SET role = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2")
                .bind(r.as_str())
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        if let Some(active) = is_active {
            sqlx::query("UPDATE users SET is_active = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2")
                .bind(active)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        if let Some(hash) = pwd_hash {
            sqlx::query("UPDATE users SET password_hash = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2")
                .bind(hash)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn update_user_permissions(&self, user_id: &str, permissions: &UserPermissions) -> Result<()> {
        sqlx::query(
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
        )
        .bind(user_id)
        .bind(permissions.can_download)
        .bind(permissions.can_manage_favorites)
        .bind(permissions.can_trigger_catalog_sync)
        .bind(permissions.can_force_refresh)
        .bind(permissions.can_view_modules)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_user_last_login(&self, user_id: &str) -> Result<()> {
        sqlx::query("UPDATE users SET last_login_at = CURRENT_TIMESTAMP WHERE id = ?1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_user(&self, user_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM users WHERE id = ?1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
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

        sqlx::query(
            r#"
            INSERT INTO user_sessions (id, token_hash, user_id, created_at, expires_at, last_active_at, user_agent, ip_address)
            VALUES (?1, ?2, ?3, ?4, ?5, ?4, ?6, ?7)
            "#,
        )
        .bind(id)
        .bind(token_hash)
        .bind(user_id)
        .bind(now)
        .bind(expires_at)
        .bind(user_agent)
        .bind(ip_address)
        .execute(&self.pool)
        .await?;

        Ok(raw_token)
    }

    pub async fn validate_session_or_token(&self, raw_token: &str) -> Result<Option<AuthContext>> {
        let token_hash = hash_token(raw_token);
        let now = Utc::now();

        // 1. Try finding in user_sessions
        let session_row = sqlx::query(
            r#"
            SELECT 
                s.id as session_id, s.user_id, s.expires_at, 
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject, 
                u.created_at, u.updated_at, u.last_login_at, u.is_active
            FROM user_sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = ?1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = session_row {
            let expires_at: chrono::DateTime<Utc> = r.get("expires_at");
            let is_active: bool = r.get("is_active");
            if is_active && expires_at > now {
                let session_id: String = r.get("session_id");
                let user_id: String = r.get("user_id");
                let role_str: String = r.get("role");
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
                let _ = sqlx::query("UPDATE user_sessions SET last_active_at = CURRENT_TIMESTAMP WHERE id = ?1")
                    .bind(session_id)
                    .execute(&self.pool)
                    .await;

                let permissions = self.get_user_permissions(&user_id).await?;
                return Ok(Some(AuthContext {
                    user,
                    permissions,
                    is_api_token: false,
                }));
            }
        }

        // 2. Try finding in api_tokens
        let token_row = sqlx::query(
            r#"
            SELECT 
                t.id as token_id, t.user_id, t.name, t.expires_at,
                u.id, u.username, u.password_hash, u.role, u.sso_provider, u.sso_subject, 
                u.created_at, u.updated_at, u.last_login_at, u.is_active
            FROM api_tokens t
            JOIN users u ON u.id = t.user_id
            WHERE t.token_hash = ?1
            "#,
        )
        .bind(&token_hash)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = token_row {
            let expires_at: Option<chrono::DateTime<Utc>> = r.get("expires_at");
            let is_active: bool = r.get("is_active");
            let is_valid = is_active && match expires_at {
                Some(exp) => exp > now,
                None => true,
            };

            if is_valid {
                let token_id: String = r.get("token_id");
                let user_id: String = r.get("user_id");
                let role_str: String = r.get("role");
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
                let _ = sqlx::query("UPDATE api_tokens SET last_used_at = CURRENT_TIMESTAMP WHERE id = ?1")
                    .bind(token_id)
                    .execute(&self.pool)
                    .await;

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
        sqlx::query("DELETE FROM user_sessions WHERE token_hash = ?1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
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

        sqlx::query(
            r#"
            INSERT INTO api_tokens (id, token_hash, user_id, name, created_at, expires_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&id)
        .bind(&token_hash)
        .bind(user_id)
        .bind(name)
        .bind(now)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

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

    pub async fn list_api_tokens_for_user(&self, user_id: &str) -> Result<Vec<crate::auth::ApiToken>> {
        let rows = sqlx::query(
            "SELECT id, token_hash, user_id, name, created_at, expires_at, last_used_at FROM api_tokens WHERE user_id = ?1 ORDER BY created_at DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let mut list = Vec::new();
        for r in rows {
            list.push(crate::auth::ApiToken {
                id: r.get("id"),
                token_hash: r.get("token_hash"),
                user_id: r.get("user_id"),
                name: r.get("name"),
                created_at: r.get("created_at"),
                expires_at: r.get("expires_at"),
                last_used_at: r.get("last_used_at"),
            });
        }
        Ok(list)
    }

    pub async fn delete_api_token(&self, user_id: &str, token_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM api_tokens WHERE id = ?1 AND user_id = ?2")
            .bind(token_id)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ==========================================
    // PER-MODULE CACHE & POLICIES
    // ==========================================

    pub async fn get_manga_cache(&self, manga_id: &str) -> Result<Option<MangaCacheItem>> {
        let row = sqlx::query(
            r#"
            SELECT manga_id, module_id, title, cover_url, metadata_json, chapters_json, cached_at, expires_at
            FROM manga_cache
            WHERE manga_id = ?1
            "#,
        )
        .bind(manga_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| MangaCacheItem {
            manga_id: r.get("manga_id"),
            module_id: r.get("module_id"),
            title: r.get("title"),
            cover_url: r.get("cover_url"),
            metadata_json: r.get("metadata_json"),
            chapters_json: r.get("chapters_json"),
            cached_at: r.get("cached_at"),
            expires_at: r.get("expires_at"),
        }))
    }

    pub async fn save_manga_cache(&self, item: &MangaCacheItem) -> Result<()> {
        sqlx::query(
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
        )
        .bind(&item.manga_id)
        .bind(&item.module_id)
        .bind(&item.title)
        .bind(&item.cover_url)
        .bind(&item.metadata_json)
        .bind(&item.chapters_json)
        .bind(item.cached_at)
        .bind(item.expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete_manga_cache(&self, manga_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM manga_cache WHERE manga_id = ?1")
            .bind(manga_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_module_cache_policy(&self, module_id: &str) -> Result<ModuleCachePolicy> {
        let row = sqlx::query(
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies WHERE module_id = ?1"
        )
        .bind(module_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = row {
            let unit_str: String = r.get("ttl_unit");
            return Ok(ModuleCachePolicy {
                module_id: r.get("module_id"),
                ttl_value: r.get::<i64, _>("ttl_value") as u32,
                ttl_unit: TtlUnit::from_str_loose(&unit_str),
                min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                updated_at: r.get("updated_at"),
            });
        }

        // Fallback to global policy "*"
        let fallback_row = sqlx::query(
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies WHERE module_id = '*'"
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = fallback_row {
            let unit_str: String = r.get("ttl_unit");
            return Ok(ModuleCachePolicy {
                module_id: module_id.to_string(),
                ttl_value: r.get::<i64, _>("ttl_value") as u32,
                ttl_unit: TtlUnit::from_str_loose(&unit_str),
                min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                updated_at: r.get("updated_at"),
            });
        }

        Ok(ModuleCachePolicy {
            module_id: module_id.to_string(),
            ..Default::default()
        })
    }

    pub async fn list_module_cache_policies(&self) -> Result<Vec<ModuleCachePolicy>> {
        let rows = sqlx::query(
            "SELECT module_id, ttl_value, ttl_unit, min_delay_ms, updated_at FROM module_cache_policies ORDER BY module_id ASC"
        )
        .fetch_all(&self.pool)
        .await?;

        let mut list = Vec::new();
        for r in rows {
            let unit_str: String = r.get("ttl_unit");
            list.push(ModuleCachePolicy {
                module_id: r.get("module_id"),
                ttl_value: r.get::<i64, _>("ttl_value") as u32,
                ttl_unit: TtlUnit::from_str_loose(&unit_str),
                min_delay_ms: r.get::<i64, _>("min_delay_ms") as u64,
                updated_at: r.get("updated_at"),
            });
        }
        Ok(list)
    }

    pub async fn set_module_cache_policy(&self, policy: &ModuleCachePolicy) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO module_cache_policies (module_id, ttl_value, ttl_unit, min_delay_ms, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(module_id) DO UPDATE SET
                ttl_value = excluded.ttl_value,
                ttl_unit = excluded.ttl_unit,
                min_delay_ms = excluded.min_delay_ms,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&policy.module_id)
        .bind(policy.ttl_value as i64)
        .bind(policy.ttl_unit.as_str())
        .bind(policy.min_delay_ms as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_init() {
        let db = Database::new(":memory:").await.unwrap();
        // Check that tables exist
        let row: (i64,) = sqlx::query_as("SELECT count(*) FROM mangas")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(row.0, 0);

        // Test catalog upsert and search
        let items = vec![
            ("Jujutsu Kaisen".to_string(), "novel/Jujutsu-Kaisen.html".to_string()),
            ("One Piece".to_string(), "novel/One-Piece.html".to_string()),
            ("Chainsaw Man".to_string(), "novel/Chainsaw-Man.html".to_string()),
        ];
        let count = db.upsert_catalog_items("NovelCoolFR", &items).await.unwrap();
        assert_eq!(count, 3);

        let (search_res, total) = db.search_catalog("jujutsu", None, 10, 0).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(search_res[0].title, "Jujutsu Kaisen");

        // Test module sync status
        db.set_module_sync_status("NovelCoolFR", "Completed", None, Some(3)).await.unwrap();
        let status = db.get_module_sync_status("NovelCoolFR").await.unwrap().unwrap();
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
            crate::models::ChapterItem { title: "Ch 1".into(), url: "ch1".into() },
            crate::models::ChapterItem { title: "Ch 2".into(), url: "ch2".into() },
        ];
        // Record initial chapters marked as already seen/downloaded
        db.record_chapters("m1", &initial_chapters, true).await.unwrap();

        let favs = db.list_favorites_with_stats().await.unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].total_chapters, 2);
        assert_eq!(favs[0].downloaded_chapters_count, 2);
        assert_eq!(favs[0].auto_download, true);

        // Simulate chapter update: Ch 3 is released!
        let updated_chapters = vec![
            crate::models::ChapterItem { title: "Ch 1".into(), url: "ch1".into() },
            crate::models::ChapterItem { title: "Ch 2".into(), url: "ch2".into() },
            crate::models::ChapterItem { title: "Ch 3".into(), url: "ch3".into() },
        ];
        let new_chaps = db.check_favorite_new_chapters("m1", &updated_chapters).await.unwrap();
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
        let admin = db.create_initial_admin("superadmin", "P@ssw0rd123!").await.unwrap();
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
        assert_eq!(perms.can_trigger_catalog_sync, false);

        // 4. Session authentication
        let session_token = db.create_session(&user.id, Some("Firefox/130"), Some("127.0.0.1"), 7).await.unwrap();
        let auth_ctx = db.validate_session_or_token(&session_token).await.unwrap().expect("session should be valid");
        assert_eq!(auth_ctx.user.id, user.id);
        assert_eq!(auth_ctx.permissions.can_download, true);
        assert_eq!(auth_ctx.is_api_token, false);

        // Delete session
        db.delete_session(&session_token).await.unwrap();
        assert!(db.validate_session_or_token(&session_token).await.unwrap().is_none());

        // 5. API Token authentication
        let (raw_api_token, _token_meta) = db.create_api_token(&admin.id, "ci-token", Some(30)).await.unwrap();
        let api_ctx = db.validate_session_or_token(&raw_api_token).await.unwrap().expect("api token should be valid");
        assert_eq!(api_ctx.user.id, admin.id);
        assert_eq!(api_ctx.is_api_token, true);

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

        let cached = db.get_manga_cache("mangadex:12345").await.unwrap().expect("cache should exist");
        assert_eq!(cached.title, "Chainsaw Man");
        assert!(!cached.is_expired());
    }
}
