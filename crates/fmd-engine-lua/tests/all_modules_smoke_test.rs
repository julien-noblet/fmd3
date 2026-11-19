use fmd_engine_lua::LuaModuleEngine;
use std::path::Path;

#[test]
fn smoke_test_all_lua_modules_load_cleanly() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let modules_dir = Path::new(&manifest_dir).join("../../lua/modules");
    assert!(
        modules_dir.exists(),
        "Expected lua/modules directory at {}",
        modules_dir.display()
    );

    let engine = LuaModuleEngine::new().expect("LuaModuleEngine init");
    let modules = engine
        .scan_modules_dir(&modules_dir)
        .expect("scan_modules_dir must not fail");

    assert!(
        modules.len() >= 400,
        "Expected at least 400 modules to load cleanly, got {}",
        modules.len()
    );

    for m in &modules {
        assert!(
            !m.id.trim().is_empty(),
            "Module {:?} has empty id",
            m.file_path
        );
        assert!(
            !m.name.trim().is_empty(),
            "Module {:?} has empty name",
            m.file_path
        );
        assert!(
            !m.root_url.trim().is_empty(),
            "Module {} ({}) has empty root_url",
            m.name,
            m.id
        );
    }
}
