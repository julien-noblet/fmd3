use anyhow::{bail, Context};
use fmd_engine_lua::WebsiteModule;
use std::collections::HashSet;

pub const SFW_BLOCKED_MESSAGE: &str = "Adult content is disabled in SFW mode";

const ADULT_CATEGORIES: &[&str] = &["h-sites", "adult"];

const ADULT_MODULE_KEYWORDS: &[&str] = &[
    "hentai",
    "porn",
    "adult",
    "erotic",
    "erotica",
    "nsfw",
    "milf",
    "smut",
    "doujin",
    "18kami",
    "18comic",
    "manga18",
    "manhwa18",
    "ero18x",
    "pururin",
    "luscious",
    "tsumino",
    "fakku",
    "mangacraberos",
    "eros.mangacrab.org",
    "manhwasmen",
    "manhwas.men",
    "manhwasnet",
    "manhwas.net",
    ".xxx",
];

const ADULT_GENRES: &[&str] = &[
    "hentai",
    "adult",
    "adulte",
    "adulto",
    "pornographic",
    "pornographique",
    "porn",
    "porno",
    "erotica",
    "erotic",
    "érotique",
    "erotique",
    "smut",
    "ecchi",
    "mature",
    "18+",
    "+18",
    "r18",
    "r-18",
    "nc-17",
    "nsfw",
];

/// Parses `FMD_SFW_MODE` from an optional raw value, defaulting to `demo_mode` when unset or empty.
pub fn parse(raw: Option<&str>, demo_mode: bool) -> anyhow::Result<bool> {
    match raw.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(demo_mode),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(other) => bail!("Invalid FMD_SFW_MODE value {other:?}: expected true or false"),
    }
}

/// Reads `FMD_SFW_MODE` from the environment, defaulting to `demo_mode` when unset or empty.
pub fn from_env(demo_mode: bool) -> anyhow::Result<bool> {
    let raw = match std::env::var("FMD_SFW_MODE") {
        Ok(val) => Some(val),
        Err(std::env::VarError::NotPresent) => None,
        Err(err) => return Err(err).context("Invalid FMD_SFW_MODE"),
    };
    parse(raw.as_deref(), demo_mode)
}

/// Returns true if a Lua website module is dedicated to adult or hentai content.
pub fn is_adult_module(module: &WebsiteModule) -> bool {
    let category = module.category.trim().to_ascii_lowercase();
    if ADULT_CATEGORIES.contains(&category.as_str()) {
        return true;
    }

    let name = module.name.to_ascii_lowercase();
    let root_url = module.root_url.to_ascii_lowercase();
    ADULT_MODULE_KEYWORDS
        .iter()
        .any(|kw| name.contains(kw) || root_url.contains(kw))
}

/// Filters out adult modules and returns `(sfw_modules, blocked_module_ids)`.
pub fn filter_modules(modules: Vec<WebsiteModule>) -> (Vec<WebsiteModule>, HashSet<String>) {
    let mut sfw = Vec::with_capacity(modules.len());
    let mut blocked = HashSet::new();
    for module in modules {
        if is_adult_module(&module) {
            blocked.insert(module.id);
        } else {
            sfw.push(module);
        }
    }
    (sfw, blocked)
}

/// Returns true if a genre/tag string represents adult, hentai, ecchi, or mature content.
pub fn is_adult_genre(genre: &str) -> bool {
    let normalized = genre.trim().to_lowercase();
    ADULT_GENRES.contains(&normalized.as_str())
}

/// Returns true if any genre/tag in the slice is blocked in SFW mode.
pub fn has_adult_genre(genres: &[String]) -> bool {
    genres.iter().any(|g| is_adult_genre(g))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(id: &str, name: &str, root_url: &str, category: &str) -> WebsiteModule {
        WebsiteModule {
            id: id.into(),
            name: name.into(),
            root_url: root_url.into(),
            category: category.into(),
            file_path: format!("lua/modules/{name}.lua"),
            ..Default::default()
        }
    }

    #[test]
    fn parse_defaults_to_demo_mode_when_unset_or_empty() {
        assert!(!parse(None, false).unwrap());
        assert!(parse(None, true).unwrap());
        assert!(parse(Some("  "), true).unwrap());
        assert!(!parse(Some("false"), true).unwrap());
        assert!(parse(Some("true"), false).unwrap());
        assert!(parse(Some("invalid"), false).is_err());
    }

    #[test]
    fn detects_adult_modules_by_category_name_or_url() {
        assert!(is_adult_module(&module(
            "1",
            "NHentai",
            "https://nhentai.net",
            "H-Sites"
        )));
        assert!(is_adult_module(&module(
            "2",
            "PornComic",
            "https://porncomic.com",
            "Adult"
        )));
        assert!(is_adult_module(&module(
            "3",
            "MangaWorldAdult",
            "https://www.mangaworldadult.net",
            "Italian"
        )));
        assert!(is_adult_module(&module(
            "4",
            "MangaCrabEros",
            "https://eros.mangacrab.org",
            "Spanish"
        )));
        assert!(is_adult_module(&module(
            "5",
            "ManhwasMen",
            "https://manhwas.men",
            "Raw"
        )));
        assert!(!is_adult_module(&module(
            "6",
            "MangaDex",
            "https://mangadex.org",
            "English"
        )));
        assert!(!is_adult_module(&module(
            "15994152f69b4b8188cc93e8b13d4f34",
            "MangaPark",
            "https://mangapark.net",
            "English"
        )));
    }

    #[test]
    fn detects_strict_adult_genres() {
        assert!(has_adult_genre(&["Action".into(), "Hentai".into()]));
        assert!(has_adult_genre(&["Ecchi".into()]));
        assert!(has_adult_genre(&["Mature".into()]));
        assert!(has_adult_genre(&["Érotique".into()]));
        assert!(has_adult_genre(&["Pornographic".into()]));
        assert!(has_adult_genre(&["18+".into()]));
        assert!(!has_adult_genre(&["Action".into(), "Adventure".into()]));
    }
}
