use anyhow::{bail, Context, Result};

pub fn from_env() -> Result<String> {
    match std::env::var("BASE_PATH") {
        Ok(value) => normalize(&value),
        Err(std::env::VarError::NotPresent) => Ok(String::new()),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("BASE_PATH must be valid UTF-8")
        }
    }
}

fn normalize(value: &str) -> Result<String> {
    if value.trim() != value
        || value.starts_with("//")
        || value.contains(['?', '#', '\\'])
        || value.contains("://")
    {
        bail!("BASE_PATH must be a path only, without a URL, query, or fragment");
    }

    let path = value.trim_matches('/');
    if path.is_empty() {
        return Ok(String::new());
    }

    if path.split('/').any(|segment| {
        segment.is_empty()
            || segment == "."
            || segment == ".."
            || !segment
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~'))
    }) {
        bail!("BASE_PATH contains an invalid path segment");
    }

    format!("/{path}")
        .parse::<axum::http::uri::PathAndQuery>()
        .context("BASE_PATH is not a valid URL path")?;
    Ok(format!("/{path}"))
}

#[cfg(test)]
mod tests {
    use super::normalize;

    #[test]
    fn normalizes_path_prefixes() {
        for (input, expected) in [
            ("reader", "/reader"),
            ("/reader", "/reader"),
            ("/reader/", "/reader"),
            ("", ""),
            ("/", ""),
        ] {
            assert_eq!(normalize(input).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_non_path_values() {
        for input in [
            "https://example.com/reader",
            "reader?tab=search",
            "reader#section",
            "//example.com",
            "reader/../admin",
            "reader//admin",
            "reader name",
        ] {
            assert!(normalize(input).is_err(), "{input:?} should be rejected");
        }
    }
}
