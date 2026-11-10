//! Multilingual i18n support for FMD3 San (三)

pub const LANGUAGES_JSON: &str = include_str!("../../../locales/languages.json");

pub const ALL_LOCALES: &[(&str, &str)] = &[
    ("ca", include_str!("../../../locales/ca.json")),
    ("cs", include_str!("../../../locales/cs.json")),
    ("de", include_str!("../../../locales/de.json")),
    ("el", include_str!("../../../locales/el.json")),
    ("en", include_str!("../../../locales/en.json")),
    ("es", include_str!("../../../locales/es.json")),
    ("fi", include_str!("../../../locales/fi.json")),
    ("fr", include_str!("../../../locales/fr.json")),
    ("he", include_str!("../../../locales/he.json")),
    ("hu", include_str!("../../../locales/hu.json")),
    ("id", include_str!("../../../locales/id.json")),
    ("it", include_str!("../../../locales/it.json")),
    ("ja", include_str!("../../../locales/ja.json")),
    ("lt", include_str!("../../../locales/lt.json")),
    ("nl", include_str!("../../../locales/nl.json")),
    ("no", include_str!("../../../locales/no.json")),
    ("pl", include_str!("../../../locales/pl.json")),
    ("pt", include_str!("../../../locales/pt.json")),
    ("pt_BR", include_str!("../../../locales/pt_BR.json")),
    ("ru", include_str!("../../../locales/ru.json")),
    ("sk", include_str!("../../../locales/sk.json")),
    ("tr", include_str!("../../../locales/tr.json")),
    ("uk", include_str!("../../../locales/uk.json")),
    ("zh", include_str!("../../../locales/zh.json")),
    ("zh_CN", include_str!("../../../locales/zh_CN.json")),
];

/// Produce a self-contained JavaScript block defining `const LOCALES` and `const LANGUAGES`
pub fn get_locales_js() -> String {
    let mut s = String::from("const LOCALES = {\n");
    for (idx, (code, json)) in ALL_LOCALES.iter().enumerate() {
        if idx > 0 {
            s.push_str(",\n");
        }
        s.push_str(&format!("  \"{}\": {}", code, json.trim()));
    }
    s.push_str("\n};\nconst LANGUAGES = ");
    s.push_str(LANGUAGES_JSON.trim());
    s.push_str(";\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_locales_valid_json() {
        for (code, raw) in ALL_LOCALES {
            let val: serde_json::Value = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("Failed to parse locale {}: {}", code, e));
            assert!(val.get("nav").is_some(), "Locale {} missing 'nav'", code);
            assert!(val.get("common").is_some(), "Locale {} missing 'common'", code);
            assert!(val.get("auth").is_some(), "Locale {} missing 'auth'", code);
            assert!(val.get("stats").is_some(), "Locale {} missing 'stats'", code);
            assert!(val.get("favorites").is_some(), "Locale {} missing 'favorites'", code);
            assert!(val.get("catalog").is_some(), "Locale {} missing 'catalog'", code);
            assert!(val.get("settings").is_some(), "Locale {} missing 'settings'", code);
        }
    }

    #[test]
    fn test_all_locales_100_percent_parity() {
        use std::collections::HashSet;

        fn extract_keys(val: &serde_json::Value, prefix: &str, set: &mut HashSet<String>) {
            if let Some(obj) = val.as_object() {
                for (k, v) in obj {
                    let full = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    if v.is_object() {
                        extract_keys(v, &full, set);
                    } else {
                        set.insert(full);
                    }
                }
            }
        }

        // Canonical keys from en
        let (_, en_raw) = ALL_LOCALES
            .iter()
            .find(|(code, _)| *code == "en")
            .expect("en locale must exist");
        let en_val: serde_json::Value = serde_json::from_str(en_raw).unwrap();
        let mut en_keys = HashSet::new();
        extract_keys(&en_val, "", &mut en_keys);
        assert!(!en_keys.is_empty(), "en locale must have keys");

        for (code, raw) in ALL_LOCALES {
            let val: serde_json::Value = serde_json::from_str(raw)
                .unwrap_or_else(|e| panic!("Failed to parse locale {}: {}", code, e));
            let mut keys = HashSet::new();
            extract_keys(&val, "", &mut keys);

            let missing: Vec<_> = en_keys.difference(&keys).collect();
            assert!(
                missing.is_empty(),
                "Locale '{}' is missing {} keys: {:?}",
                code,
                missing.len(),
                missing
            );
            assert_eq!(
                keys.len(),
                en_keys.len(),
                "Locale '{}' key count {} != en key count {}",
                code,
                keys.len(),
                en_keys.len()
            );
        }
    }

    #[test]
    fn test_languages_json_matches() {
        let val: Vec<serde_json::Value> = serde_json::from_str(LANGUAGES_JSON).unwrap();
        assert_eq!(val.len(), ALL_LOCALES.len());
    }

    #[test]
    fn test_get_locales_js() {
        let js = get_locales_js();
        assert!(js.starts_with("const LOCALES = {"));
        assert!(js.contains("\"en\": {"));
        assert!(js.contains("\"fr\": {"));
        assert!(js.contains("\"ja\": {"));
        assert!(js.contains("const LANGUAGES = ["));
    }

    #[test]
    fn test_all_dom_and_js_i18n_keys_exist_in_locales() {
        use std::collections::HashSet;

        fn extract_keys(val: &serde_json::Value, prefix: &str, set: &mut HashSet<String>) {
            if let Some(obj) = val.as_object() {
                for (k, v) in obj {
                    let full = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    if v.is_object() {
                        extract_keys(v, &full, set);
                    } else {
                        set.insert(full);
                    }
                }
            }
        }

        let (_, en_raw) = ALL_LOCALES
            .iter()
            .find(|(code, _)| *code == "en")
            .expect("en locale must exist");
        let en_val: serde_json::Value = serde_json::from_str(en_raw).unwrap();
        let mut en_keys = HashSet::new();
        extract_keys(&en_val, "", &mut en_keys);

        let html = crate::render_app_html();
        for attr in ["data-i18n=\"", "data-i18n-placeholder=\"", "data-i18n-title=\""] {
            let mut rest = html.as_str();
            while let Some(pos) = rest.find(attr) {
                rest = &rest[pos + attr.len()..];
                if let Some(end) = rest.find('"') {
                    let key = &rest[..end];
                    assert!(
                        en_keys.contains(key),
                        "HTML attribute {} references missing i18n key '{}'",
                        attr,
                        key
                    );
                    rest = &rest[end + 1..];
                }
            }
        }

        let js = crate::CLIENT_SCRIPT;
        for pattern in ["t('", "t(\""] {
            let quote = pattern.chars().last().unwrap();
            let mut rest = js;
            while let Some(pos) = rest.find(pattern) {
                // Ensure `t` is not part of an identifier (e.g. `split('` or `getAttribute("`)
                let valid_Boundary = if pos == 0 {
                    true
                } else {
                    let prev = rest[..pos].chars().last().unwrap();
                    !prev.is_alphanumeric() && prev != '_' && prev != '$'
                };
                rest = &rest[pos + pattern.len()..];
                if valid_Boundary {
                    if let Some(end) = rest.find(quote) {
                        let key = &rest[..end];
                        if key.contains('.') && !key.contains(' ') {
                            assert!(
                                en_keys.contains(key),
                                "CLIENT_SCRIPT t() call references missing i18n key '{}'",
                                key
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_all_non_english_locales_are_translated() {
        use std::collections::HashMap;

        fn extract_map(val: &serde_json::Value, prefix: &str, map: &mut HashMap<String, String>) {
            if let Some(obj) = val.as_object() {
                for (k, v) in obj {
                    let full = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    if v.is_object() {
                        extract_map(v, &full, map);
                    } else if let Some(s) = v.as_str() {
                        map.insert(full, s.to_string());
                    }
                }
            }
        }

        let (_, en_raw) = ALL_LOCALES
            .iter()
            .find(|(code, _)| *code == "en")
            .unwrap();
        let en_val: serde_json::Value = serde_json::from_str(en_raw).unwrap();
        let mut en_map = HashMap::new();
        extract_map(&en_val, "", &mut en_map);

        for (code, raw) in ALL_LOCALES {
            if *code == "en" {
                continue;
            }
            let val: serde_json::Value = serde_json::from_str(raw).unwrap();
            let mut loc_map = HashMap::new();
            extract_map(&val, "", &mut loc_map);

            let total = en_map.len();
            let mut translated = 0;
            for (k, en_str) in &en_map {
                let loc_str = loc_map.get(k).unwrap();
                assert!(!loc_str.trim().is_empty(), "Locale '{}' has empty value for '{}'", code, k);
                if loc_str != en_str {
                    translated += 1;
                }
            }
            let ratio = (translated as f64) / (total as f64);
            assert!(
                ratio >= 0.85,
                "Locale '{}' only has {:.1}% distinct strings from English ({}/{})",
                code,
                ratio * 100.0,
                translated,
                total
            );
        }
    }
}
