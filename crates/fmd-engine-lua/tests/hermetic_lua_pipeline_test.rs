use fmd_core::{ChapterMetadata, ChapterPacker, Database, PackagingFormat};
use fmd_engine_lua::LuaModuleEngine;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0x63,
    0x34, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[tokio::test(flavor = "multi_thread")]
async fn hermetic_lua_full_scraping_packaging_db_and_catalog_cancel_pipeline() {
    let server = MockServer::start().await;
    let base = server.uri();

    // Catalog pages
    Mock::given(method("GET"))
        .and(path("/catalog/0"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"<div class="pager" data-pages="2"></div>
               <a class="item" href="/manga/one">One Manga</a>
               <a class="item" href="/manga/two">Two Manga</a>"#,
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/catalog/1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"<a class="item" href="/manga/three">Three Manga</a>"#),
        )
        .mount(&server)
        .await;

    // Manga info page
    Mock::given(method("GET"))
        .and(path("/manga/one"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"<html><body>
                <h1 class="title">One Manga</h1>
                <span class="author">Test Author</span>
                <span class="status">Ongoing</span>
                <span class="genre">Action</span>
                <span class="genre">Fantasy</span>
                <p class="summary">Hermetic summary</p>
                <img class="cover" src="{base}/cover.png" />
                <div class="chapters">
                    <a href="/ch/1">Chapitre 1</a>
                </div>
            </body></html>"#
        )))
        .mount(&server)
        .await;

    // Chapter pages
    Mock::given(method("GET"))
        .and(path("/ch/1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            r#"<img class="page" src="{base}/img/1.png" />
               <img class="page" src="{base}/img/2.custom" />"#
        )))
        .mount(&server)
        .await;

    // Standard image with Referer check
    Mock::given(method("GET"))
        .and(path("/img/1.png"))
        .and(header("Referer", &format!("{base}/")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
        .mount(&server)
        .await;

    // Custom-downloaded image (base64 payload decoded by Lua OnDownloadImage)
    use base64::Engine as _;
    let b64_png = base64::engine::general_purpose::STANDARD.encode(PNG_1X1);
    Mock::given(method("GET"))
        .and(path("/img/2.custom"))
        .respond_with(ResponseTemplate::new(200).set_body_string(b64_png))
        .mount(&server)
        .await;

    let tmp_dir = tempfile::tempdir().unwrap();
    let module_path = tmp_dir.path().join("HermeticMod.lua");
    std::fs::write(
        &module_path,
        format!(
            r#"
local crypto = require('fmd.crypto')

function Init()
    local m = NewWebsiteModule()
    m.ID = 'hermetic-mod'
    m.Name = 'HermeticMod'
    m.RootURL = '{base}'
    m.Category = 'French'
    m.TotalDirectory = 1
    m.OnGetNameAndLink = 'GetNameAndLink'
    m.OnGetInfo = 'GetInfo'
    m.OnGetPageNumber = 'GetPageNumber'
    m.OnBeforeDownloadImage = 'BeforeDownloadImage'
    m.OnDownloadImage = 'DownloadImage'
end

function GetNameAndLink()
    if HTTP.GET(MODULE.RootURL .. '/catalog/' .. URL) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        x.XPathHREFAll('//a[@class="item"]', LINKS, NAMES)
        local p = tonumber(x.XPathString('//div[@class="pager"]/@data-pages'))
        if p and p > 1 then
            UPDATELIST.CurrentDirectoryPageNumber = p
        end
        return no_error
    end
    return net_problem
end

function GetInfo()
    if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        MANGAINFO.Title = x.XPathString('//h1[@class="title"]')
        MANGAINFO.Authors = x.XPathString('//span[@class="author"]')
        MANGAINFO.Status = MangaInfoStatusIfPos(x.XPathString('//span[@class="status"]'))
        MANGAINFO.Summary = x.XPathString('//p[@class="summary"]')
        MANGAINFO.CoverLink = x.XPathString('//img[@class="cover"]/@src')
        MANGAINFO.Genres = x.XPathStringAll('//span[@class="genre"]')
        x.XPathHREFAll('//div[@class="chapters"]/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
        MANGAINFO.AlternateVersions.Add('en', 'One Manga EN', MODULE.RootURL .. '/en/one')
        return no_error
    end
    return net_problem
end

function GetPageNumber()
    if HTTP.GET(MaybeFillHost(MODULE.RootURL, URL)) then
        local x = CreateTXQuery(HTTP.Document.ToString())
        x.XPathStringAll('//img[@class="page"]/@src', TASK.PageLinks)
        return no_error
    end
    return net_problem
end

function BeforeDownloadImage()
    HTTP.Headers.Values['Referer'] = MODULE.RootURL .. '/'
    return true
end

function DownloadImage()
    if string.find(URL, '%.custom$') then
        if HTTP.GET(URL) then
            local decoded = crypto.DecodeBase64(HTTP.Document.ToString())
            HTTP.Document.WriteString(decoded)
            return true
        end
        return false
    end
    return nil
end
"#
        ),
    )
    .unwrap();

    let engine = LuaModuleEngine::new().unwrap();
    let modules = engine.scan_modules_dir(tmp_dir.path()).unwrap();
    assert_eq!(modules.len(), 1);
    let mod_path_str = &modules[0].file_path;

    // 1. Full catalog sync (2 pages -> 3 items)
    let cancel_false = Arc::new(AtomicBool::new(false));
    let mut collected = Vec::new();
    let total = engine
        .update_catalog(mod_path_str, cancel_false, |entries, _prog| {
            collected.extend(entries);
            Ok(())
        })
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(collected.len(), 3);

    // 2. Catalog sync cancellation after page 0
    let cancel_after_one = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel_after_one.clone();
    let mut cancelled_items = 0;
    let total_cancelled = engine
        .update_catalog(mod_path_str, cancel_after_one, |entries, _prog| {
            cancelled_items += entries.len();
            cancel_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .unwrap();
    assert_eq!(total_cancelled, 2);
    assert_eq!(cancelled_items, 2);

    // 3. Manga info extraction + DB persistence
    let info = engine
        .get_manga_info(mod_path_str, &format!("{base}/manga/one"))
        .unwrap();
    assert_eq!(info.title, "One Manga");
    assert_eq!(info.authors.as_deref(), Some("Test Author"));
    assert_eq!(info.genres, vec!["Action", "Fantasy"]);
    assert_eq!(info.alternate_versions.len(), 1);
    assert_eq!(info.chapters.len(), 1);

    let db = Database::new_in_memory().await.unwrap();
    db.upsert_catalog_items(
        "hermetic-mod",
        &collected
            .into_iter()
            .map(|e| (e.title, e.link))
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    assert_eq!(db.get_catalog_total_count().await.unwrap(), 3);

    // 4. Pages + prepare_download_image + custom download + CBZ packing
    let pages = engine
        .get_chapter_pages(mod_path_str, &info.chapters[0].url)
        .unwrap();
    assert_eq!(pages.len(), 2);

    let headers0 = engine
        .prepare_download_image(mod_path_str, &pages[0], 0)
        .unwrap();
    assert_eq!(
        headers0.get("Referer").map(String::as_str),
        Some(format!("{base}/").as_str())
    );
    let page0_bytes = engine
        .http_client()
        .get_bytes_with_headers(&pages[0], &headers0)
        .await
        .unwrap();
    assert_eq!(page0_bytes, PNG_1X1);

    let page1_bytes = engine
        .download_custom_image(mod_path_str, &pages[1], 1)
        .unwrap()
        .expect("custom image bytes expected");
    assert_eq!(page1_bytes, PNG_1X1);

    let cbz_path = tmp_dir.path().join("one_ch1.cbz");
    let mut packer = ChapterPacker::new(info.title.clone(), info.chapters[0].title.clone());
    packer.set_metadata(ChapterMetadata {
        series: info.title,
        title: info.chapters[0].title.clone(),
        number: Some("1".into()),
        summary: info.summary,
        writer: Some("Test Author".into()),
        genres: info.genres,
        web: Some(format!("{base}/manga/one")),
        language_iso: Some("fr".into()),
    });
    packer.add_page("001.png".into(), page0_bytes);
    packer.add_page("002.png".into(), page1_bytes);
    packer.pack_to(PackagingFormat::Cbz, &cbz_path).unwrap();

    let mut zip = zip::ZipArchive::new(std::fs::File::open(&cbz_path).unwrap()).unwrap();
    assert_eq!(zip.len(), 3);
    let mut comic_xml = String::new();
    zip.by_name("ComicInfo.xml")
        .unwrap()
        .read_to_string(&mut comic_xml)
        .unwrap();
    assert!(comic_xml.contains("<Series>One Manga</Series>"));
    assert!(comic_xml.contains("<Writer>Test Author</Writer>"));
}
