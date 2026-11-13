use std::sync::LazyLock;

static INDEX_HTML: LazyLock<String> = LazyLock::new(fmd_ui::render_app_html);

pub fn index_html() -> &'static str {
    &INDEX_HTML
}

pub fn render_index_html(base_path: &str) -> String {
    fmd_ui::render_app_html_with_base_path(base_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_html_rendered_via_leptos() {
        let html = index_html();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("FMD3"));
        assert!(html.contains("tab-favorites"));
    }

    #[test]
    fn rendered_html_uses_base_path() {
        let html = render_index_html("/reader");
        assert!(html.contains("href=\"/reader/favicon.ico\""));
        assert!(html.contains("src=\"/reader/logo.png\""));
        assert!(html.contains("const BASE_PATH = \"/reader\";"));
    }
}
