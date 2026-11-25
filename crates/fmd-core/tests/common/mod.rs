use fmd_core::auth::{Role, UserPermissions};
use fmd_core::cache::{MangaCacheItem, ModuleCachePolicy, TtlUnit};
use fmd_core::models::{AppSettings, ChapterItem};
use fmd_core::Database;

/// Shared integration contract suite executed against both SQLite and PostgreSQL backends.
pub async fn run_database_contract_suite(db: &Database) {
    // 1. Idempotent migrations
    db.migrate().await.expect("second migrate() must succeed");
    assert_eq!(db.count_mangas().await.unwrap(), 0);
    assert_eq!(db.count_chapters().await.unwrap(), 0);

    // 2. Catalog upsert, full-text search, pagination, module filtering, special chars
    let catalog_items = vec![
        ("Jujutsu Kaisen".to_string(), "novel/jjk.html".to_string()),
        ("One Piece".to_string(), "novel/op.html".to_string()),
        ("One Punch Man".to_string(), "novel/opm.html".to_string()),
        ("Chainsaw Man".to_string(), "novel/csm.html".to_string()),
        (
            "Spy x Family: Mission!".to_string(),
            "novel/sxf.html".to_string(),
        ),
    ];
    let inserted = db
        .upsert_catalog_items("mod-alpha", &catalog_items)
        .await
        .unwrap();
    assert_eq!(inserted, 5);

    // Upsert duplicate should update rather than duplicate
    db.upsert_catalog_items(
        "mod-alpha",
        &[("One Piece".to_string(), "novel/op.html".to_string())],
    )
    .await
    .unwrap();
    db.upsert_catalog_items(
        "mod-beta",
        &[("One Piece Remake".to_string(), "beta/opr.html".to_string())],
    )
    .await
    .unwrap();
    assert_eq!(db.get_catalog_total_count().await.unwrap(), 6);

    // Empty query browse + pagination + module filter
    let (page1, total_alpha) = db
        .search_catalog("", Some("mod-alpha"), 2, 0)
        .await
        .unwrap();
    assert_eq!(total_alpha, 5);
    assert_eq!(page1.len(), 2);
    let (page2, _) = db
        .search_catalog("", Some("mod-alpha"), 2, 2)
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert_ne!(page1[0].id, page2[0].id);

    // Full-text search + special punctuation
    let (one_hits, one_total) = db.search_catalog("One", None, 10, 0).await.unwrap();
    assert!(one_total >= 3);
    assert!(one_hits.iter().any(|i| i.title == "One Piece"));
    let (one_alpha, one_alpha_total) = db
        .search_catalog("One", Some("mod-alpha"), 10, 0)
        .await
        .unwrap();
    assert_eq!(one_alpha_total, 2);
    assert!(one_alpha.iter().all(|i| i.module_id == "mod-alpha"));

    let (punct_hits, _) = db
        .search_catalog("Spy! \"Family\"", None, 10, 0)
        .await
        .unwrap();
    assert!(punct_hits.iter().any(|i| i.title.contains("Spy x Family")));

    // Module sync status
    db.set_module_sync_status("mod-alpha", "Syncing", None, Some(2))
        .await
        .unwrap();
    db.set_module_sync_status("mod-alpha", "Completed", None, Some(5))
        .await
        .unwrap();
    let sync = db
        .get_module_sync_status("mod-alpha")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sync.status, "Completed");
    assert_eq!(sync.total_mangas, 5);
    let all_syncs = db.list_all_sync_statuses().await.unwrap();
    assert!(all_syncs.iter().any(|s| s.module_id == "mod-alpha"));

    // 3. Users, RBAC permissions, sessions, API tokens, expiration & ownership
    assert_eq!(db.count_users().await.unwrap(), 0);
    // Legacy unassigned favorite before admin setup
    db.add_or_update_favorite(
        "mod-alpha:legacy",
        "mod-alpha",
        "Legacy Series",
        "https://example.com/legacy",
        None,
        false,
        "CBZ",
    )
    .await
    .unwrap();

    let admin = db
        .create_initial_admin("contract_admin", "AdminPass123!")
        .await
        .unwrap();
    assert_eq!(admin.role, Role::Admin);
    assert_eq!(db.count_users().await.unwrap(), 1);

    let alice = db
        .create_user(
            "alice",
            Some("AlicePass123!"),
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
    let bob = db
        .create_user("bob", Some("BobPass123!"), Role::User, None, None, None)
        .await
        .unwrap();

    assert!(db.get_user_by_id(&alice.id).await.unwrap().is_some());
    assert!(db.get_user_by_username("alice").await.unwrap().is_some());
    assert_eq!(db.list_users_with_permissions().await.unwrap().len(), 3);

    // Update user role, permissions & active flag
    db.update_user(&bob.id, Some(Role::Admin), None, Some("BobNewPass!"))
        .await
        .unwrap();
    let bob_updated = db.get_user_by_id(&bob.id).await.unwrap().unwrap();
    assert_eq!(bob_updated.role, Role::Admin);

    let mut alice_perms = db.get_user_permissions(&alice.id).await.unwrap();
    assert!(!alice_perms.can_trigger_catalog_sync);
    alice_perms.can_trigger_catalog_sync = true;
    db.update_user_permissions(&alice.id, &alice_perms)
        .await
        .unwrap();
    assert!(
        db.get_user_permissions(&alice.id)
            .await
            .unwrap()
            .can_trigger_catalog_sync
    );

    // Sessions: active, expired, inactive user
    let valid_sess = db
        .create_session(&alice.id, Some("UA"), Some("127.0.0.1"), 7)
        .await
        .unwrap();
    let expired_sess = db
        .create_session(&alice.id, Some("UA"), Some("127.0.0.1"), -1)
        .await
        .unwrap();
    assert!(db
        .validate_session_or_token(&valid_sess)
        .await
        .unwrap()
        .is_some());
    assert!(db
        .validate_session_or_token(&expired_sess)
        .await
        .unwrap()
        .is_none());

    // Deactivating user invalidates active session
    db.update_user(&alice.id, None, Some(false), None)
        .await
        .unwrap();
    assert!(db
        .validate_session_or_token(&valid_sess)
        .await
        .unwrap()
        .is_none());
    db.update_user(&alice.id, None, Some(true), None)
        .await
        .unwrap();
    db.delete_session(&valid_sess).await.unwrap();
    assert!(db
        .validate_session_or_token(&valid_sess)
        .await
        .unwrap()
        .is_none());

    // API tokens: valid, expired, ownership guard on delete
    let (alice_tok, alice_meta) = db
        .create_api_token(&alice.id, "alice-cli", Some(30))
        .await
        .unwrap();
    let (expired_tok, _) = db
        .create_api_token(&alice.id, "expired-cli", Some(-1))
        .await
        .unwrap();
    assert!(
        db.validate_session_or_token(&alice_tok)
            .await
            .unwrap()
            .unwrap()
            .is_api_token
    );
    assert!(db
        .validate_session_or_token(&expired_tok)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        db.list_api_tokens_for_user(&alice.id).await.unwrap().len(),
        2
    );

    // Bob cannot delete Alice's token
    db.delete_api_token(&bob.id, &alice_meta.id).await.unwrap();
    assert!(db
        .validate_session_or_token(&alice_tok)
        .await
        .unwrap()
        .is_some());
    db.delete_api_token(&alice.id, &alice_meta.id)
        .await
        .unwrap();
    assert!(db
        .validate_session_or_token(&alice_tok)
        .await
        .unwrap()
        .is_none());

    // 4. Multi-user favorites isolation, chapter tracking & reconciliation
    db.add_or_update_favorite_with_meta_for_user(
        Some(&alice.id),
        "mod-alpha:alice-manga",
        "mod-alpha",
        "Alice Manga",
        "https://example.com/alice",
        Some("https://example.com/alice.jpg"),
        Some("Author A"),
        Some("Desc A"),
        Some(&["Action".to_string()]),
        true,
        "CBZ",
    )
    .await
    .unwrap();
    db.add_or_update_favorite_with_meta_for_user(
        Some(&bob.id),
        "mod-alpha:bob-manga",
        "mod-alpha",
        "Bob Manga",
        "https://example.com/bob",
        None,
        None,
        None,
        None,
        false,
        "PDF",
    )
    .await
    .unwrap();

    let alice_favs = db.list_favorites_for_user(Some(&alice.id)).await.unwrap();
    assert!(alice_favs
        .iter()
        .any(|f| f.manga_id == "mod-alpha:alice-manga"));
    assert!(!alice_favs
        .iter()
        .any(|f| f.manga_id == "mod-alpha:bob-manga"));

    let bob_favs = db.list_favorites_for_user(Some(&bob.id)).await.unwrap();
    assert!(bob_favs.iter().any(|f| f.manga_id == "mod-alpha:bob-manga"));
    assert!(!bob_favs
        .iter()
        .any(|f| f.manga_id == "mod-alpha:alice-manga"));

    db.update_favorite_settings("mod-alpha:alice-manga", false, false, "EPUB")
        .await
        .unwrap();
    let alice_fav_after = db.list_favorites_for_user(Some(&alice.id)).await.unwrap();
    let updated_fav = alice_fav_after
        .iter()
        .find(|f| f.manga_id == "mod-alpha:alice-manga")
        .unwrap();
    assert!(!updated_fav.enabled);
    assert!(!updated_fav.auto_download);
    assert_eq!(updated_fav.download_format, "EPUB");

    let chaps = vec![
        ChapterItem {
            title: "Ch 1".into(),
            url: "https://example.com/alice/1".into(),
        },
        ChapterItem {
            title: "Ch 2".into(),
            url: "https://example.com/alice/2".into(),
        },
    ];
    db.record_chapters("mod-alpha:alice-manga", &chaps, true)
        .await
        .unwrap();
    assert_eq!(
        db.list_manga_chapters("mod-alpha:alice-manga")
            .await
            .unwrap()
            .len(),
        2
    );
    let new_chaps = db
        .check_favorite_new_chapters(
            "mod-alpha:alice-manga",
            &[
                chaps[0].clone(),
                chaps[1].clone(),
                ChapterItem {
                    title: "Ch 3".into(),
                    url: "https://example.com/alice/3".into(),
                },
            ],
        )
        .await
        .unwrap();
    assert_eq!(new_chaps.len(), 1);
    db.mark_chapter_downloaded("mod-alpha:alice-manga", "https://example.com/alice/3")
        .await
        .unwrap();

    // 5. Downloads queue lifecycle, retries, reconciliation
    let dl1 = format!("dl-{}", uuid::Uuid::new_v4());
    let dl2 = format!("dl-{}", uuid::Uuid::new_v4());
    db.queue_download(
        &dl1,
        "Alice Manga",
        "Ch 1",
        "https://example.com/alice/1",
        "mod-alpha",
        "CBZ",
        3,
    )
    .await
    .unwrap();
    db.queue_download(
        &dl2,
        "Alice Manga",
        "Ch 2",
        "https://example.com/alice/2",
        "mod-alpha",
        "CBZ",
        3,
    )
    .await
    .unwrap();

    assert_eq!(db.list_pending_downloads().await.unwrap().len(), 2);
    let next = db.fetch_next_pending_download().await.unwrap().unwrap();
    assert_eq!(next.0, dl1);
    assert_eq!(
        db.get_download_chapter_url(&dl1).await.unwrap().as_deref(),
        Some("https://example.com/alice/1")
    );

    db.set_download_status(&dl1, "Downloading").await.unwrap();
    db.set_download_total_pages(&dl1, 10).await.unwrap();
    db.set_download_progress(&dl1, 5).await.unwrap();
    db.update_download_progress_full(&dl1, "Downloading", 7, Some(10), None)
        .await
        .unwrap();
    db.mark_download_completed_full(&dl1).await.unwrap();

    // dl2: retry -> failed -> reset
    db.schedule_download_retry(&dl2, 1, "Tentative 1/3")
        .await
        .unwrap();
    let retry_info = db.get_download_retry_info(&dl2).await.unwrap().unwrap();
    assert_eq!(retry_info.6, 1);
    db.mark_download_failed(&dl2, "HTTP 500").await.unwrap();
    assert_eq!(db.list_failed_downloads().await.unwrap().len(), 1);
    assert_eq!(db.list_incomplete_downloads().await.unwrap().len(), 1);

    let reset_count = db.reset_all_failed_downloads().await.unwrap();
    assert_eq!(reset_count, 1);
    db.mark_download_failed(&dl2, "HTTP 502").await.unwrap();
    db.reset_download_for_retry(&dl2).await.unwrap();
    assert_eq!(
        db.get_download_status_and_retry(&dl2)
            .await
            .unwrap()
            .unwrap()
            .0,
        "Pending"
    );

    assert_eq!(db.list_downloads_summary(10).await.unwrap().len(), 2);
    assert_eq!(
        db.list_downloads_with_created_at(10).await.unwrap().len(),
        2
    );

    // Reconcile marks ch2/ch3 as not downloaded because only dl1 (ch1) is Completed
    let reconciled = db.reconcile_downloaded_chapters().await.unwrap();
    assert!(reconciled >= 1);

    db.delete_download(&dl1).await.unwrap();
    db.delete_download(&dl2).await.unwrap();
    db.delete_favorite("mod-alpha:alice-manga").await.unwrap();
    db.delete_favorite("mod-alpha:bob-manga").await.unwrap();
    db.delete_favorite("mod-alpha:legacy").await.unwrap();

    // 6. Settings, cache policies, manga cache
    let custom_settings = AppSettings {
        download_dir: "custom_dl".into(),
        default_format: "EPUB".into(),
        naming_template: "{manga}/{number}".into(),
        flaresolverr_url: "http://flare:8191/v1".into(),
        check_interval_hours: 6,
        catalog_resync_days: 14,
    };
    db.update_settings(&custom_settings).await.unwrap();
    let fetched_settings = db.get_settings().await.unwrap();
    assert_eq!(fetched_settings.download_dir, "custom_dl");
    assert_eq!(fetched_settings.default_format, "EPUB");
    assert_eq!(fetched_settings.check_interval_hours, 6);

    db.set_module_cache_policy(&ModuleCachePolicy {
        module_id: "mod-alpha".into(),
        ttl_value: 12,
        ttl_unit: TtlUnit::Hours,
        min_delay_ms: 1500,
        updated_at: chrono::Utc::now(),
    })
    .await
    .unwrap();
    assert!(!db.list_module_cache_policies().await.unwrap().is_empty());

    let cache_item = MangaCacheItem {
        manga_id: "mod-alpha:cached".into(),
        module_id: "mod-alpha".into(),
        title: "Cached Title".into(),
        cover_url: None,
        metadata_json: "{}".into(),
        chapters_json: "[]".into(),
        cached_at: chrono::Utc::now(),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    db.save_manga_cache(&cache_item).await.unwrap();
    assert!(db
        .get_manga_cache("mod-alpha:cached")
        .await
        .unwrap()
        .is_some());
    db.delete_manga_cache("mod-alpha:cached").await.unwrap();
    assert!(db
        .get_manga_cache("mod-alpha:cached")
        .await
        .unwrap()
        .is_none());

    // 7. Legacy FMD2 SQLite import into target DB
    let tmp_dir = tempfile::tempdir().unwrap();
    let legacy_path = tmp_dir.path().join("userdata.db");
    {
        let legacy_url = format!("sqlite://{}?mode=rwc", legacy_path.display());
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&legacy_url)
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
                'leg-1', 1, 1, 'mod-legacy', 'https://legacy.example/m1',
                'Legacy Imported Manga', 'ongoing', 'Ch 5', '/tmp/save', '', '', ''
            );
            INSERT INTO downloadedchapters VALUES ('leg-1', 'Ch 1' || char(10) || 'Ch 2');
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    let imported_count = db.import_legacy_db(&legacy_path).await.unwrap();
    assert_eq!(imported_count, 1);
    let legacy_chaps = db.list_manga_chapters("leg-1").await.unwrap();
    assert_eq!(legacy_chaps.len(), 2);

    // Cleanup users
    db.delete_user(&alice.id).await.unwrap();
    db.delete_user(&bob.id).await.unwrap();
}
