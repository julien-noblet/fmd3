pub mod app;
pub mod client_script;
pub mod components;
pub mod i18n;
pub mod views;

pub use app::App;
pub use client_script::CLIENT_SCRIPT;
pub use components::Navbar;
pub use i18n::{get_locales_js, ALL_LOCALES, LANGUAGES_JSON};
pub use views::*;

use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

/// Render full HTML single-page application via Leptos SSR
pub fn render_app_html() -> String {
    render_app_html_with_base_path("")
}

pub fn render_app_html_with_base_path(base_path: &str) -> String {
    let body = (view! { <App base_path=base_path.to_string() /> }).to_html();
    let locales_js = get_locales_js();
    let script = client_script::render(base_path);

    format!(
        r#"<!DOCTYPE html>
<html lang="en" class="dark">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta name="referrer" content="no-referrer">
    <link rel="icon" type="image/x-icon" href="{base_path}/favicon.ico">
    <link rel="icon" type="image/png" sizes="32x32" href="{base_path}/favicon-32x32.png">
    <link rel="icon" type="image/png" sizes="16x16" href="{base_path}/favicon-16x16.png">
    <link rel="apple-touch-icon" sizes="180x180" href="{base_path}/apple-touch-icon.png">
    <title>FMD3 — Free Manga Downloader San (三)</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script>
        tailwind.config = {{
            darkMode: 'class',
            theme: {{
                extend: {{
                    colors: {{
                        brand: {{ 500: '#ef4444', 600: '#dc2626' }}
                    }}
                }}
            }}
        }}
    </script>
    <style>
        ::-webkit-scrollbar {{ width: 8px; height: 8px; }}
        ::-webkit-scrollbar-track {{ background: #18181b; }}
        ::-webkit-scrollbar-thumb {{ background: #3f3f46; border-radius: 4px; }}
    </style>
</head>
<body class="bg-zinc-950 text-zinc-100 min-h-screen flex flex-col font-sans">
    {body}
    <script>
        {locales_js}
        {script}
    </script>
</body>
</html>"#,
        body = body,
        locales_js = locales_js,
        base_path = base_path,
        script = script
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_app_html_output() {
        let html = render_app_html();
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("FMD3"));
        assert!(html.contains("href=\"/favicon.ico\""));
        assert!(html.contains("src=\"/logo.png\""));
        assert!(html.contains("const BASE_PATH = \"\";"));
        assert!(html.contains("id=\"tab-favorites\""));
        assert!(html.contains("id=\"tab-catalog\""));
        assert!(html.contains("id=\"tab-search\""));
        assert!(html.contains("id=\"tab-merger\""));
        assert!(html.contains("id=\"tab-downloads\""));
        assert!(html.contains("id=\"tab-modules\""));
        assert!(html.contains("id=\"tab-settings\""));
        // Critical element IDs for JS client script
        assert!(html.contains("id=\"favorites-list\""));
        assert!(html.contains("id=\"modules-list\""));
        assert!(html.contains("id=\"fetch-module-select\""));
        assert!(html.contains("id=\"catalog-module-select\""));
        assert!(html.contains("id=\"downloads-table-body\""));
        assert!(html.contains("id=\"stat-favorites\""));
        assert!(html.contains("id=\"stat-modules\""));
        // Browser history and routing support
        assert!(html.contains("popstate"));
        assert!(html.contains("hashchange"));
        assert!(html.contains("initRouting"));
        // Multilingual i18n support
        assert!(html.contains("const LOCALES = {"));
        assert!(html.contains("const LANGUAGES = ["));
        assert!(html.contains("id=\"nav-lang-select\""));
        assert!(html.contains("id=\"auth-lang-select\""));
        assert!(html.contains("id=\"settings-lang-select\""));
        assert!(html.contains("initI18n"));
        assert!(html.contains("setLocale"));
        assert!(html.contains("data-i18n=\"nav.favorites\""));

        // Validate JavaScript syntax and execution using node if node is present
        if let Ok(mut child) = std::process::Command::new("node")
            .arg("-e")
            .arg(
                r#"
const vm = require('vm');
let code = '';
process.stdin.on('data', d => code += d);
process.stdin.on('end', async () => {
    const sandbox = {
        window: {},
        document: { getElementById: () => null, querySelectorAll: () => [], documentElement: {} },
        navigator: { language: 'fr' },
        localStorage: { getItem: () => null, setItem: () => null },
        fetch: async () => ({ status: 401, ok: false, json: async () => ({}) }),
        WebSocket: class { static OPEN = 1; constructor() { this.readyState = 1; } },
        console: console,
        setTimeout: () => {},
        clearTimeout: () => {},
        history: { replaceState: () => {}, pushState: () => {} },
        addEventListener: () => {},
        location: { protocol: 'http:', host: 'localhost:8088', hash: '' },
        LOCALES: { en: {}, fr: {} },
        LANGUAGES: []
    };
    sandbox.window = sandbox;
    try {
        vm.createContext(sandbox);
        vm.runInContext(code, sandbox);
        if (sandbox.withBasePath('/api/health') !== '/reader/api/health') {
            throw new Error('API URLs should include BASE_PATH');
        }
        if (sandbox.logout) {
            await sandbox.logout();
        }
        process.exit(0);
    } catch (e) {
        console.error('JS Execution failed:', e);
        process.exit(1);
    }
});
"#,
            )
            .stdin(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(client_script::render("/reader").as_bytes())
                    .unwrap();
            }
            let output = child.wait_with_output().unwrap();
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                output.status.success(),
                "CLIENT_SCRIPT has JavaScript execution/runtime errors:\n{}",
                stderr
            );
        }
    }

    #[test]
    fn test_individual_views_ssr() {
        let navbar = (view! { <Navbar /> }).to_html();
        assert!(navbar.contains("FMD3"));
        assert!(navbar.contains("tab-btn-favorites"));
        assert!(navbar.contains("tab-btn-modules"));

        let library = (view! { <LibraryView /> }).to_html();
        assert!(library.contains("favorites-list"));

        let catalog = (view! { <CatalogView /> }).to_html();
        assert!(catalog.contains("catalog-search-input"));
        assert!(catalog.contains("catalog-table-body"));

        let merger = (view! { <MergerView /> }).to_html();
        assert!(merger.contains("merger-title-input"));

        let downloads = (view! { <DownloadsView /> }).to_html();
        assert!(downloads.contains("downloads-table-body"));

        let modules = (view! { <ModulesView /> }).to_html();
        assert!(modules.contains("modules-list"));

        let settings = (view! { <SettingsView /> }).to_html();
        assert!(settings.contains("setting-download-dir"));
        assert!(settings.contains("btn-save-settings"));
    }
}
