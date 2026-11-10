use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WebsiteModule {
    pub id: String,
    pub name: String,
    pub root_url: String,
    pub category: String,
    pub file_path: String,
    pub on_get_name_and_link: Option<String>,
    pub on_get_info: Option<String>,
    pub on_get_page_number: Option<String>,
    pub on_get_image_url: Option<String>,
    pub on_before_download_image: Option<String>,
    pub on_download_image: Option<String>,
    pub on_get_directory_page_number: Option<String>,
    pub total_directory: usize,
}
