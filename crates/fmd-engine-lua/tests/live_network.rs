use fmd_engine_lua::LuaModuleEngine;

#[test]
#[ignore = "requires live external network access"]
fn test_lelscan_xpath_diagnostic() {
    let engine = LuaModuleEngine::new().unwrap();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/LelscanFR.lua");
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

#[tokio::test]
#[ignore = "requires live external network access"]
async fn test_novelcool_fr_get_info() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

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
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

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
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/NovelCoolFR.lua");

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

#[tokio::test]
#[ignore = "requires live external network access"]
async fn test_sushiscan_cover_fetch() {
    let client = fmd_net::HttpClient::new().unwrap();
    let mut headers = std::collections::HashMap::new();
    headers.insert("Referer".into(), "https://sushiscan.net/".into());
    let bytes = client
        .get_bytes_with_headers(
            "https://sushiscan.net/wp-content/uploads/RoadToLaughTaleCover.png",
            &headers,
        )
        .await
        .expect("fetch sushiscan cover failed");
    assert!(!bytes.is_empty(), "Fetched cover bytes must not be empty");
}

#[tokio::test]
#[ignore = "requires live external network access"]
async fn test_mangaplus_get_info() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/MangaPlus.lua");

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
    let module_path = std::path::Path::new(&manifest_dir).join("../../lua/modules/MangaPlus.lua");

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
