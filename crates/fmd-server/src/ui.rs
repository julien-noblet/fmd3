use std::sync::LazyLock;

pub static INDEX_HTML: LazyLock<String> = LazyLock::new(fmd_ui::render_app_html);

pub fn index_html() -> &'static str {
    &INDEX_HTML
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
}
