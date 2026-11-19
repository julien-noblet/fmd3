mod common;

use fmd_core::Database;

#[tokio::test]
async fn sqlite_in_memory_contract_suite() {
    let db = Database::new_in_memory().await.unwrap();
    common::run_database_contract_suite(&db).await;
    db.close().await;
}

#[tokio::test]
async fn sqlite_file_persistence_contract_suite() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("fmd_contract.db");

    {
        let db = Database::new(&db_path).await.unwrap();
        common::run_database_contract_suite(&db).await;
        db.set_setting("download_dir", "persisted-dir")
            .await
            .unwrap();
        db.close().await;
    }

    // Reopen file database and verify persistence
    let reopened = Database::new(&db_path).await.unwrap();
    assert_eq!(
        reopened
            .get_setting("download_dir")
            .await
            .unwrap()
            .as_deref(),
        Some("persisted-dir")
    );
    assert!(reopened
        .list_favorites_with_stats()
        .await
        .unwrap()
        .iter()
        .any(|f| f.manga_id == "leg-1"));
    reopened.close().await;
}
