use crate::errors::Result;
use crate::models::{ChapterMetadata, PackagingFormat};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
use regex::Regex;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub struct ChapterPacker {
    pub manga_title: String,
    pub chapter_title: String,
    pub pages: Vec<(String, Vec<u8>)>, // (filename, image_data)
    pub metadata: Option<ChapterMetadata>,
}

impl ChapterPacker {
    pub fn new(manga_title: String, chapter_title: String) -> Self {
        Self {
            manga_title,
            chapter_title,
            pages: Vec::new(),
            metadata: None,
        }
    }

    pub fn set_metadata(&mut self, metadata: ChapterMetadata) {
        self.metadata = Some(metadata);
    }

    pub fn add_page(&mut self, filename: String, data: Vec<u8>) {
        self.pages.push((filename, data));
    }

    pub fn generate_comic_info_xml(&self, page_count: usize) -> String {
        let meta = self.metadata.as_ref();
        let series = meta.map(|m| m.series.as_str()).unwrap_or(&self.manga_title);
        let title = meta.map(|m| m.title.as_str()).unwrap_or(&self.chapter_title);
        let number = meta
            .and_then(|m| m.number.clone())
            .unwrap_or_else(|| extract_chapter_number(&self.chapter_title).unwrap_or_default());
        let summary = meta.and_then(|m| m.summary.as_deref()).unwrap_or("");
        let writer = meta.and_then(|m| m.writer.as_deref()).unwrap_or("");
        let genres = meta.map(|m| m.genres.join(", ")).unwrap_or_default();
        let web = meta.and_then(|m| m.web.as_deref()).unwrap_or("");
        let lang = meta.and_then(|m| m.language_iso.as_deref()).unwrap_or("fr");

        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Title>{}</Title>
  <Series>{}</Series>
  <Number>{}</Number>
  <Summary>{}</Summary>
  <Writer>{}</Writer>
  <Genre>{}</Genre>
  <Tags>{}</Tags>
  <Web>{}</Web>
  <LanguageISO>{}</LanguageISO>
  <Manga>YesAndRightToLeft</Manga>
  <PageCount>{}</PageCount>
</ComicInfo>"#,
            escape_xml(title),
            escape_xml(series),
            escape_xml(&number),
            escape_xml(summary),
            escape_xml(writer),
            escape_xml(&genres),
            escape_xml(&genres),
            escape_xml(web),
            escape_xml(lang),
            page_count
        )
    }

    pub fn pack_to(&self, format: PackagingFormat, output_path: &Path) -> Result<()> {
        match format {
            PackagingFormat::Directory => self.pack_to_directory(output_path),
            PackagingFormat::Cbz | PackagingFormat::Zip => self.pack_to_zip(output_path),
            PackagingFormat::Pdf => self.pack_to_pdf(output_path),
            PackagingFormat::Epub => self.pack_to_epub(output_path),
        }
    }

    fn pack_to_directory(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        for (name, data) in &self.pages {
            let path = dir.join(name);
            std::fs::write(path, data)?;
        }
        let comic_info_xml = self.generate_comic_info_xml(self.pages.len());
        let _ = std::fs::write(dir.join("ComicInfo.xml"), comic_info_xml);
        Ok(())
    }

    fn pack_to_zip(&self, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(output_path)?;
        let mut zip = ZipWriter::new(BufWriter::new(file));
        let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        for (name, data) in &self.pages {
            zip.start_file(name, options)?;
            zip.write_all(data)?;
        }

        // Add ComicInfo.xml for CBZ / ZIP
        let comic_info_xml = self.generate_comic_info_xml(self.pages.len());
        zip.start_file("ComicInfo.xml", options)?;
        zip.write_all(comic_info_xml.as_bytes())?;

        zip.finish()?;
        Ok(())
    }

    fn pack_to_pdf(&self, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut pdf = Pdf::new();
        let mut page_refs = Vec::new();

        let mut next_ref = 1;
        let mut alloc_ref = || {
            let r = Ref::new(next_ref);
            next_ref += 1;
            r
        };

        let catalog_ref = alloc_ref();
        let pages_ref = alloc_ref();

        for (_idx, (_name, data)) in self.pages.iter().enumerate() {
            if let Ok(img) = image::load_from_memory(data) {
                let width = img.width() as f32;
                let height = img.height() as f32;

                let page_ref = alloc_ref();
                let image_ref = alloc_ref();
                let content_ref = alloc_ref();

                page_refs.push(page_ref);

                let mut page = pdf.page(page_ref);
                page.media_box(Rect::new(0.0, 0.0, width, height));
                page.parent(pages_ref);
                page.contents(content_ref);

                let mut resources = page.resources();
                resources.x_objects().pair(Name(b"Im0"), image_ref);
                resources.finish();
                page.finish();

                // Draw image on page
                let mut content = Content::new();
                content.save_state();
                content.transform([width, 0.0, 0.0, height, 0.0, 0.0]);
                content.x_object(Name(b"Im0"));
                content.restore_state();
                pdf.stream(content_ref, &content.finish());

                // Embed image as JPEG or raw RGB
                let rgb = img.to_rgb8();
                let mut image_stream = pdf.image_xobject(image_ref, &rgb);
                image_stream.width(img.width() as i32);
                image_stream.height(img.height() as i32);
                image_stream.color_space().device_rgb();
                image_stream.bits_per_component(8);
                image_stream.finish();
            }
        }

        let mut pages = pdf.pages(pages_ref);
        pages.kids(page_refs.iter().copied());
        pages.count(page_refs.len() as i32);
        pages.finish();

        let mut catalog = pdf.catalog(catalog_ref);
        catalog.pages(pages_ref);
        catalog.finish();

        let pdf_bytes = pdf.finish();
        std::fs::write(output_path, pdf_bytes)?;
        Ok(())
    }

    fn pack_to_epub(&self, output_path: &Path) -> Result<()> {
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(output_path)?;
        let mut zip = ZipWriter::new(BufWriter::new(file));

        // 1. mimetype (MUST be first file, uncompressed / Stored)
        let stored_opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("mimetype", stored_opt)?;
        zip.write_all(b"application/epub+zip")?;

        let deflated_opt = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        // 2. META-INF/container.xml
        zip.start_file("META-INF/container.xml", deflated_opt)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )?;

        // Metadata fields
        let title = format!("{} - {}", self.manga_title, self.chapter_title);
        let author = self
            .metadata
            .as_ref()
            .and_then(|m| m.writer.as_deref())
            .unwrap_or("FMD3 San");
        let lang = self
            .metadata
            .as_ref()
            .and_then(|m| m.language_iso.as_deref())
            .unwrap_or("fr");

        // 3. Store images and pages
        let mut manifest_items = String::new();
        let mut spine_items = String::new();

        for (idx, (name, data)) in self.pages.iter().enumerate() {
            let img_filename = format!("images/{}", name);
            let img_zip_path = format!("OEBPS/{}", img_filename);
            zip.start_file(&img_zip_path, deflated_opt)?;
            zip.write_all(data)?;

            let img_id = format!("img_{:04}", idx + 1);
            let page_id = format!("page_{:04}", idx + 1);
            let page_filename = format!("page_{:04}.xhtml", idx + 1);
            let page_zip_path = format!("OEBPS/{}", page_filename);

            let mime = if name.ends_with(".png") {
                "image/png"
            } else if name.ends_with(".webp") {
                "image/webp"
            } else {
                "image/jpeg"
            };

            manifest_items.push_str(&format!(
                "    <item id=\"{}\" href=\"{}\" media-type=\"{}\"/>\n",
                img_id, img_filename, mime
            ));
            manifest_items.push_str(&format!(
                "    <item id=\"{}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
                page_id, page_filename
            ));
            spine_items.push_str(&format!("    <itemref idref=\"{}\"/>\n", page_id));

            let xhtml = format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Page {}</title></head>
<body style="margin: 0; padding: 0; text-align: center; background: #000;">
<img src="{}" style="max-width: 100%; height: auto;" />
</body>
</html>"#,
                idx + 1,
                img_filename
            );

            zip.start_file(&page_zip_path, deflated_opt)?;
            zip.write_all(xhtml.as_bytes())?;
        }

        // 4. OEBPS/content.opf
        let opf = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookID" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{}</dc:title>
    <dc:creator>{}</dc:creator>
    <dc:language>{}</dc:language>
  </metadata>
  <manifest>
{}  </manifest>
  <spine>
{}  </spine>
</package>"#,
            escape_xml(&title),
            escape_xml(author),
            escape_xml(lang),
            manifest_items,
            spine_items
        );

        zip.start_file("OEBPS/content.opf", deflated_opt)?;
        zip.write_all(opf.as_bytes())?;

        zip.finish()?;
        Ok(())
    }
}

pub fn extract_chapter_number(title: &str) -> Option<String> {
    let re = Regex::new(r"(?i)(?:chapitre|chapter|ch\.?|ep\.?|volume|vol\.?|tome)?\s*(\d+(?:\.\d+)?)").ok()?;
    if let Some(caps) = re.captures(title) {
        if let Some(m) = caps.get(1) {
            return Some(m.as_str().to_string());
        }
    }
    None
}

pub fn format_download_path(
    base_dir: &Path,
    template: &str,
    manga: &str,
    chapter: &str,
    number: &str,
    module: &str,
    ext: &str,
) -> PathBuf {
    let clean = |s: &str| -> String {
        let mut cleaned: String = s
            .chars()
            .map(|c| {
                if "\\:*?\"<>|/\0\r\n".contains(c) || c.is_control() {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        while cleaned.contains("..") {
            cleaned = cleaned.replace("..", "_");
        }
        let trimmed = cleaned.trim();
        let stripped = trimmed.trim_matches('.');
        if stripped.is_empty() {
            "_".to_string()
        } else {
            stripped.to_string()
        }
    };

    let safe_manga = clean(manga);
    let safe_chapter = clean(chapter);
    let safe_number = clean(number);
    let safe_module = clean(module);

    let pattern = if template.trim().is_empty() {
        "{manga}/{chapter}"
    } else {
        template
    };

    let resolved = pattern
        .replace("{manga}", &safe_manga)
        .replace("{chapter}", &safe_chapter)
        .replace("{number}", &safe_number)
        .replace("{module}", &safe_module);

    let mut path = base_dir.to_path_buf();
    for part in resolved.split('/') {
        let trimmed = part.trim().trim_matches('.');
        if !trimmed.is_empty() && trimmed != ".." && trimmed != "." {
            path.push(trimmed);
        }
    }

    if ext.is_empty() {
        path
    } else {
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let clean_ext = ext.trim_start_matches('.').trim();
        path.set_file_name(format!("{}.{}", file_name, clean_ext));
        path
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_chapter_number() {
        assert_eq!(extract_chapter_number("Chapitre 1180"), Some("1180".into()));
        assert_eq!(extract_chapter_number("Ch. 86 Chapitre 86"), Some("86".into()));
        assert_eq!(extract_chapter_number("1 VF"), Some("1".into()));
        assert_eq!(extract_chapter_number("Volume 4"), Some("4".into()));
        assert_eq!(extract_chapter_number("95.5"), Some("95.5".into()));
    }

    #[test]
    fn test_format_download_path() {
        let base = Path::new("downloads");
        let path1 = format_download_path(base, "{manga}/{chapter}", "One Piece", "Chapitre 1180", "1180", "SushiScan", "cbz");
        assert_eq!(path1, Path::new("downloads/One Piece/Chapitre 1180.cbz"));

        let path2 = format_download_path(base, "{manga}/{manga} - Ch.{number}", "One Piece", "Chapitre 1180", "1180", "SushiScan", "cbz");
        assert_eq!(path2, Path::new("downloads/One Piece/One Piece - Ch.1180.cbz"));

        let path3 = format_download_path(base, "{manga}/{chapter}", "One Piece", "Chapitre 1180", "1180", "SushiScan", "");
        assert_eq!(path3, Path::new("downloads/One Piece/Chapitre 1180"));
    }

    #[test]
    fn test_format_download_path_prevents_path_traversal() {
        let base = Path::new("downloads");
        let evil_path = format_download_path(
            base,
            "{manga}/{chapter}",
            "../../../../etc",
            "../../../cron.d/malicious",
            "0",
            "EvilModule",
            "cbz",
        );
        // Ensure path remains inside downloads/
        assert!(evil_path.starts_with(base));
        let path_str = evil_path.to_string_lossy();
        assert!(!path_str.contains(".."));
    }

    #[test]
    fn test_comic_info_xml_generation() {
        let mut packer = ChapterPacker::new("One Piece".into(), "Chapitre 1180".into());
        packer.set_metadata(ChapterMetadata {
            series: "One Piece".into(),
            title: "Chapitre 1180".into(),
            number: Some("1180".into()),
            summary: Some("Luffy's adventure".into()),
            writer: Some("Eiichiro Oda".into()),
            genres: vec!["Action".into(), "Aventure".into()],
            web: Some("https://sushiscan.net".into()),
            language_iso: Some("fr".into()),
        });
        let xml = packer.generate_comic_info_xml(20);
        assert!(xml.contains("<Series>One Piece</Series>"));
        assert!(xml.contains("<Number>1180</Number>"));
        assert!(xml.contains("<Writer>Eiichiro Oda</Writer>"));
        assert!(xml.contains("<Summary>Luffy&apos;s adventure</Summary>"));
        assert!(xml.contains("<Genre>Action, Aventure</Genre>"));
        assert!(xml.contains("<Tags>Action, Aventure</Tags>"));
        assert!(xml.contains("<Manga>YesAndRightToLeft</Manga>"));
        assert!(xml.contains("<PageCount>20</PageCount>"));
    }
}
