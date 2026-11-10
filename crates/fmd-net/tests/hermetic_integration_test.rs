use fmd_core::{ChapterMetadata, ChapterPacker, PackagingFormat};
use fmd_net::HttpClient;
use std::io::Read;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0x63,
    0x34, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

#[tokio::test]
async fn test_hermetic_end_to_end_scraper_and_cbz_packaging() {
    let mock_server = MockServer::start().await;

    // 1. Mock Manga series HTML
    let manga_html = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test Manga Series</title></head>
    <body>
        <h1 class="title">Test Manga Series</h1>
        <div class="chapters">
            <a href="/manga/test-series/ch-1">Chapitre 1</a>
        </div>
    </body>
    </html>
    "#;

    Mock::given(method("GET"))
        .and(path("/manga/test-series"))
        .respond_with(ResponseTemplate::new(200).set_body_string(manga_html))
        .mount(&mock_server)
        .await;

    // 2. Mock Image endpoints
    Mock::given(method("GET"))
        .and(path("/images/page1.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/images/page2.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG_1X1))
        .mount(&mock_server)
        .await;

    // 3. Test HttpClient fetching HTML
    let client = HttpClient::new().expect("HttpClient init");
    let series_url = format!("{}/manga/test-series", mock_server.uri());
    let page_html = client
        .get_text(&series_url)
        .await
        .expect("fetch series HTML");
    assert!(page_html.contains("Test Manga Series"));
    assert!(page_html.contains("Chapitre 1"));

    // 4. Download images through HttpClient
    let img1_url = format!("{}/images/page1.png", mock_server.uri());
    let img2_url = format!("{}/images/page2.png", mock_server.uri());

    let img1_bytes = client.get_bytes(&img1_url).await.expect("fetch img1");
    let img2_bytes = client.get_bytes(&img2_url).await.expect("fetch img2");

    assert_eq!(img1_bytes, PNG_1X1);
    assert_eq!(img2_bytes, PNG_1X1);

    // 5. Test packaging into CBZ with ComicInfo.xml
    let mut packer = ChapterPacker::new("Test Manga Series".into(), "Chapitre 1".into());
    packer.set_metadata(ChapterMetadata {
        series: "Test Manga Series".into(),
        title: "Chapitre 1".into(),
        number: Some("1".into()),
        summary: Some("Hermetic integration test summary".into()),
        writer: Some("Test Author".into()),
        genres: vec!["Action".into(), "Aventure".into()],
        web: Some(series_url),
        language_iso: Some("fr".into()),
    });

    packer.add_page("001.png".into(), img1_bytes);
    packer.add_page("002.png".into(), img2_bytes);

    let temp_dir = std::env::temp_dir();
    let cbz_path = temp_dir.join(format!("test_archive_{}.cbz", uuid::Uuid::new_v4()));

    packer
        .pack_to(PackagingFormat::Cbz, &cbz_path)
        .expect("pack CBZ");
    assert!(cbz_path.exists());

    // 6. Verify CBZ archive contents
    let file = std::fs::File::open(&cbz_path).expect("open CBZ");
    let mut zip = zip::ZipArchive::new(file).expect("read ZIP");

    let mut found_comicinfo = false;
    let mut found_page1 = false;
    let mut found_page2 = false;
    let mut comic_info_xml = String::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).expect("zip entry");
        let name = entry.name().expect("utf8 name");
        if name == "ComicInfo.xml" {
            found_comicinfo = true;
            entry
                .read_to_string(&mut comic_info_xml)
                .expect("read ComicInfo.xml");
        } else if name == "001.png" {
            found_page1 = true;
        } else if name == "002.png" {
            found_page2 = true;
        }
    }

    assert!(
        found_comicinfo,
        "ComicInfo.xml doit être présent dans l'archive CBZ"
    );
    assert!(found_page1, "001.png doit être présent dans l'archive CBZ");
    assert!(found_page2, "002.png doit être présent dans l'archive CBZ");

    assert!(comic_info_xml.contains("<Series>Test Manga Series</Series>"));
    assert!(comic_info_xml.contains("<Number>1</Number>"));
    assert!(comic_info_xml.contains("<Writer>Test Author</Writer>"));
    assert!(comic_info_xml.contains("<Manga>YesAndRightToLeft</Manga>"));

    // Cleanup
    let _ = std::fs::remove_file(&cbz_path);
}
