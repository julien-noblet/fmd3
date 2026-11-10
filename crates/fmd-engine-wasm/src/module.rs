use crate::abi::*;
use crate::engine::WasmEngine;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct WasmMangaModule {
    pub meta: WasmModuleMeta,
    pub file_path: PathBuf,
    engine: Arc<WasmEngine>,
}

impl WasmMangaModule {
    pub fn load_from_file<P: AsRef<Path>>(engine: Arc<WasmEngine>, path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let mut instance = engine.load_module_from_file(&path_buf)?;
        let meta = instance.get_metadata()?;

        Ok(Self {
            meta,
            file_path: path_buf,
            engine,
        })
    }

    pub fn id(&self) -> &str {
        &self.meta.id
    }

    pub fn name(&self) -> &str {
        &self.meta.name
    }

    pub fn version(&self) -> &str {
        &self.meta.version
    }

    pub fn root_url(&self) -> &str {
        &self.meta.root_url
    }

    pub fn search(&self, query: &str) -> Result<Vec<WasmCatalogItem>> {
        let mut instance = self.engine.load_module_from_file(&self.file_path)?;
        let query_obj = WasmSearchQuery {
            query: query.to_string(),
        };
        let res: WasmSearchResult = instance.call_json("_fmd_search", &query_obj)?;
        Ok(res.items)
    }

    pub fn get_manga_info(&self, url: &str) -> Result<WasmMangaInfoResult> {
        let mut instance = self.engine.load_module_from_file(&self.file_path)?;
        let query_obj = WasmMangaInfoQuery {
            url: url.to_string(),
        };
        let res: WasmMangaInfoResult = instance.call_json("_fmd_get_info", &query_obj)?;
        Ok(res)
    }

    pub fn get_chapter_pages(&self, chapter_url: &str) -> Result<Vec<String>> {
        let mut instance = self.engine.load_module_from_file(&self.file_path)?;
        let query_obj = WasmPagesQuery {
            chapter_url: chapter_url.to_string(),
        };
        let res: WasmPagesResult = instance.call_json("_fmd_get_pages", &query_obj)?;
        Ok(res.pages)
    }

    pub fn process_image(&self, bytes: &[u8], filename: &str, format: &str) -> Result<Vec<u8>> {
        let mut instance = self.engine.load_module_from_file(&self.file_path)?;
        instance.process_image(bytes, filename, format)
    }
}
