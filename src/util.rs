use color_eyre::Result;
use regex_lite::Regex;
use std::borrow::Cow;
use std::path;
use std::path::{Path, PathBuf};

pub const PATH_SEPARATOR: char = std::path::MAIN_SEPARATOR;
pub const PATH_SEPARATOR_STR: &str = std::path::MAIN_SEPARATOR_STR;
pub const PATH_SEPARATOR_REGEX_ESCAPED: &str = if PATH_SEPARATOR == '/' { "/" } else { r"\\" };
pub const INVERTED_PATH_SEPARATOR: char = if PATH_SEPARATOR == '/' { '\\' } else { '/' };

pub fn normalize_path(glob: &str) -> Cow<'_, str> {
    let mut value: Cow<str> = Cow::Borrowed(glob);
    if value.contains(INVERTED_PATH_SEPARATOR) {
        value = Cow::Owned(value.replace(INVERTED_PATH_SEPARATOR, PATH_SEPARATOR_STR));
    }
    if value.contains(&format!("{PATH_SEPARATOR}{PATH_SEPARATOR}")) {
        let regex = Regex::new(&format!("{PATH_SEPARATOR_REGEX_ESCAPED}+")).unwrap();
        value = Cow::Owned(regex.replace_all(&value, PATH_SEPARATOR_STR).into_owned());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> String {
        segments.join(PATH_SEPARATOR_STR)
    }

    mod normalization {
        use super::*;

        #[test]
        fn leaves_an_already_normalized_glob_borrowed() {
            let glob = path(&["src", "*.rs"]);
            let normalized = normalize_path(&glob);

            assert_eq!(normalized, glob);
            assert!(matches!(normalized, Cow::Borrowed(_)));
        }

        #[test]
        fn converts_inverted_and_collapses_repeated_separators() {
            let glob = format!(
                "src{0}{0}nested{1}{1}{1}*.rs",
                INVERTED_PATH_SEPARATOR, PATH_SEPARATOR
            );

            assert_eq!(normalize_path(&glob), path(&["src", "nested", "*.rs"]));
        }
    }
}

pub fn find_common_path_denominator(paths: &[&Path]) -> Result<Option<PathBuf>> {
    if paths.is_empty() {
        return Ok(None);
    }

    let absolute_paths = paths
        .iter()
        .map(path::absolute)
        .collect::<std::io::Result<Vec<_>>>()?;
    
    let mut common_path = absolute_paths[0].clone();

    while absolute_paths
        .iter()
        .skip(1)
        .any(|candidate| !candidate.starts_with(&common_path))
    {
        if !common_path.pop() {
            return Ok(None);
        }
    }
    Ok(Some(common_path))
}
