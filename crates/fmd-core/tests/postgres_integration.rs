mod common;

use fmd_core::Database;

#[tokio::test]
#[ignore = "requires PostgreSQL via FMD_TEST_POSTGRES_URL"]
async fn postgres_contract_suite_and_schema_isolation() {
    let pg_url = std::env::var("FMD_TEST_POSTGRES_URL")
        .expect("FMD_TEST_POSTGRES_URL must be set to run PostgreSQL integration tests");

    let db = Database::new_demo_slot(Some(&pg_url))
        .await
        .expect("failed to connect to PostgreSQL and initialize isolated demo schema");
    assert!(db.is_postgres());

    common::run_database_contract_suite(&db).await;

    db.close().await;
    assert!(db.is_closed());
}
