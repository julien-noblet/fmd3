pub mod abi;
pub mod engine;
pub mod module;

pub use abi::*;
pub use engine::{HostState, WasmEngine, WasmInstance};
pub use module::WasmMangaModule;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const MOCK_WAT: &str = r#"
    (module
      (import "env" "host_log" (func $host_log (param i32 i32 i32)))
      (memory (export "memory") 1)

      (data (i32.const 1024) "{\"id\":\"wasm-mock\",\"name\":\"WASM Mock\",\"version\":\"1.0.0\",\"author\":\"FMD Team\",\"root_url\":\"https://mock.wasm\",\"category\":\"Test\"}")
      (data (i32.const 2048) "{\"items\":[{\"title\":\"Mocked Manga 1\",\"link\":\"https://mock.wasm/manga/1\",\"cover_url\":null}]}")
      (data (i32.const 4096) "{\"title\":\"Mocked Manga 1\",\"author\":\"Author X\",\"artist\":null,\"description\":\"A mocked manga for testing.\",\"cover_url\":null,\"genres\":[\"Action\"],\"status\":\"Ongoing\",\"chapters\":[{\"title\":\"Ch. 1\",\"url\":\"https://mock.wasm/ch/1\"}]}")
      (data (i32.const 8192) "{\"pages\":[\"https://mock.wasm/img/1.png\",\"https://mock.wasm/img/2.png\"]}")

      (func (export "_fmd_meta") (result i64)
        (i64.or
          (i64.shl (i64.const 1024) (i64.const 32))
          (i64.const 124)
        )
      )

      (func (export "_fmd_search") (param $ptr i32) (param $len i32) (result i64)
        (call $host_log (i32.const 0) (local.get $ptr) (local.get $len))
        (i64.or
          (i64.shl (i64.const 2048) (i64.const 32))
          (i64.const 90)
        )
      )

      (func (export "_fmd_get_info") (param $ptr i32) (param $len i32) (result i64)
        (i64.or
          (i64.shl (i64.const 4096) (i64.const 32))
          (i64.const 222)
        )
      )

      (func (export "_fmd_get_pages") (param $ptr i32) (param $len i32) (result i64)
        (i64.or
          (i64.shl (i64.const 8192) (i64.const 32))
          (i64.const 71)
        )
      )

      (func (export "_fmd_process_image") (param $ptr i32) (param $len i32) (result i64)
        (i64.or
          (i64.shl (i64.extend_i32_u (local.get $ptr)) (i64.const 32))
          (i64.extend_i32_u (local.get $len))
        )
      )
    )
    "#;

    #[test]
    fn test_wasm_engine_lifecycle_and_abi() {
        let engine = Arc::new(WasmEngine::new().expect("WasmEngine init"));
        let wasm_bytes = wat::parse_str(MOCK_WAT).expect("parse WAT");

        let mut instance = engine.load_module(&wasm_bytes).expect("load module");

        // 1. Metadata check
        let meta = instance.get_metadata().expect("get metadata");
        assert_eq!(meta.id, "wasm-mock");
        assert_eq!(meta.name, "WASM Mock");
        assert_eq!(meta.version, "1.0.0");
        assert_eq!(meta.root_url, "https://mock.wasm");

        // 2. Search check
        let search_res: WasmSearchResult = instance
            .call_json(
                "_fmd_search",
                &WasmSearchQuery {
                    query: "Mock".into(),
                },
            )
            .expect("search call");
        assert_eq!(search_res.items.len(), 1);
        assert_eq!(search_res.items[0].title, "Mocked Manga 1");

        // 3. Manga info check
        let info_res: WasmMangaInfoResult = instance
            .call_json(
                "_fmd_get_info",
                &WasmMangaInfoQuery {
                    url: "https://mock.wasm/manga/1".into(),
                },
            )
            .expect("manga info call");
        assert_eq!(info_res.title, "Mocked Manga 1");
        assert_eq!(info_res.author.as_deref(), Some("Author X"));
        assert_eq!(info_res.chapters.len(), 1);

        // 4. Chapter pages check
        let pages_res: WasmPagesResult = instance
            .call_json(
                "_fmd_get_pages",
                &WasmPagesQuery {
                    chapter_url: "https://mock.wasm/ch/1".into(),
                },
            )
            .expect("pages call");
        assert_eq!(pages_res.pages.len(), 2);
        assert_eq!(pages_res.pages[0], "https://mock.wasm/img/1.png");

        // 5. Image processing filter check
        let input_img = b"FAKE_RAW_IMAGE_BYTES";
        let processed = instance
            .process_image(input_img, "page001.png", "png")
            .expect("process image");
        assert_eq!(processed, input_img);
    }

    #[test]
    fn test_wasm_manga_module_high_level_api() {
        let engine = Arc::new(WasmEngine::new().expect("WasmEngine init"));
        let wasm_bytes = wat::parse_str(MOCK_WAT).expect("parse WAT");

        let temp_dir = std::env::temp_dir();
        let wasm_file = temp_dir.join(format!("test_module_{}.wasm", uuid::Uuid::new_v4()));
        std::fs::write(&wasm_file, &wasm_bytes).expect("write temp wasm");

        let module = WasmMangaModule::load_from_file(engine, &wasm_file).expect("load from file");
        assert_eq!(module.id(), "wasm-mock");
        assert_eq!(module.name(), "WASM Mock");
        assert_eq!(module.version(), "1.0.0");
        assert_eq!(module.root_url(), "https://mock.wasm");

        let items = module.search("test").expect("search");
        assert_eq!(items.len(), 1);

        let info = module
            .get_manga_info("https://mock.wasm/manga/1")
            .expect("info");
        assert_eq!(info.title, "Mocked Manga 1");

        let pages = module
            .get_chapter_pages("https://mock.wasm/ch/1")
            .expect("pages");
        assert_eq!(pages.len(), 2);

        let processed = module
            .process_image(b"PNG_DATA", "001.png", "png")
            .expect("process image");
        assert_eq!(processed, b"PNG_DATA");

        let _ = std::fs::remove_file(&wasm_file);
    }
}
