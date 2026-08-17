use color_eyre::Result;
use color_eyre::eyre::eyre;
use regex_lite::Regex;
use serde::de::Error;
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt::Formatter;

#[derive(Debug, Clone)]
pub struct PathGlob {
    raw_pattern: String,
    normalized_pattern: String,
    normalized_segments: Vec<String>,
    prefix_regex: Regex,
    regex: Regex,
}

impl std::fmt::Display for PathGlob {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.normalized_pattern)
    }
}

impl<'de> Deserialize<'de> for PathGlob {
    fn deserialize<D>(deserializer: D) -> core::result::Result<PathGlob, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(&s).map_err(D::Error::custom)
    }
}

impl PathGlob {
    pub fn new(glob: &str) -> Result<Self> {
        build_glob(glob)
    }

    /// Returns whether `prefix` matches the glob itself or a matching path can
    /// still exist beneath that relative directory.
    pub fn accepts_prefix(&self, prefix: &str) -> bool {
        let normalized_prefix = prefix.trim_end_matches(PATH_SEPARATOR);
        if normalized_prefix.is_empty() {
            // Every valid relative glob may match something below the walk root
            return true;
        }

        if self.regex.is_match(normalized_prefix) {
            return true;
        }

        // Adds a path separator at the end to ensure a directory named `src` doesn't match
        // a file named `src.rs`
        let descendant_prefix = format!("{normalized_prefix}{PATH_SEPARATOR}");
        self.prefix_regex.is_match(&descendant_prefix)
    }

    pub fn is_match(&self, url: &str) -> bool {
        self.regex.is_match(url)
    }
}

const PATH_SEPARATOR: char = std::path::MAIN_SEPARATOR;
const PATH_SEPARATOR_STR: &str = std::path::MAIN_SEPARATOR_STR;
const PATH_SEPARATOR_REGEX_ESCAPED: &str = if PATH_SEPARATOR == '/' { "/" } else { r"\\" };
const INVERTED_PATH_SEPARATOR: char = if PATH_SEPARATOR == '/' { '\\' } else { '/' };

fn build_glob(glob: &str) -> Result<PathGlob> {
    let normalized_glob = normalize_glob(glob);
    validate_glob(&normalized_glob)?;

    let regex = glob_to_regex(&normalized_glob)?;
    let prefix_regex = glob_to_prefix_regex(&normalized_glob)?;
    let normalized_segments = normalized_glob
        .split(PATH_SEPARATOR)
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    Ok(PathGlob {
        raw_pattern: glob.to_string(),
        normalized_pattern: normalized_glob.to_string(),
        normalized_segments,
        prefix_regex,
        regex,
    })
}

fn normalize_glob(glob: &str) -> Cow<'_, str> {
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

fn validate_glob(glob: &str) -> Result<()> {
    if glob.len() == 0 {
        return Err(eyre!("Glob cannot be empty"));
    }

    // For this particular glob, a relative path-only glob, it doesn't make sense to start or end with a path separator
    if glob.starts_with(PATH_SEPARATOR) {
        return Err(eyre!("Glob cannot start with path separator"));
    }
    if glob.ends_with(PATH_SEPARATOR) {
        return Err(eyre!("Glob cannot end with path separator"));
    }

    Ok(())
}

const MATCH_ANYTHING: &str = ".*?";
const MATCH_ONE_SEGMENT: &str = if PATH_SEPARATOR == '/' {
    r"[^/]*?"
} else {
    r"[^\\/]*?"
};
const MATCH_ONE_CHAR: &str = if PATH_SEPARATOR == '/' {
    r"[^/]"
} else {
    r"[^\\/]"
};

enum GlobToken {
    Literal(char),
    OneChar,
    OneSegment,
    Anything,
}

fn glob_tokens(glob: &str) -> Vec<GlobToken> {
    let mut tokens = Vec::with_capacity(glob.len());
    let mut characters = glob.chars().peekable();

    while let Some(current) = characters.next() {
        match current {
            '?' => tokens.push(GlobToken::OneChar),
            '*' if characters.peek() == Some(&'*') => {
                tokens.push(GlobToken::Anything);
                while characters.peek() == Some(&'*') {
                    characters.next();
                }
            }
            '*' => tokens.push(GlobToken::OneSegment),
            _ => tokens.push(GlobToken::Literal(current)),
        }
    }

    tokens
}

fn push_token_regex(regex_pattern: &mut String, token: &GlobToken) {
    match token {
        GlobToken::Literal(character) => {
            if is_regex_meta_character(*character) {
                regex_pattern.push('\\');
            }
            regex_pattern.push(*character);
        }
        GlobToken::OneChar => regex_pattern.push_str(MATCH_ONE_CHAR),
        GlobToken::OneSegment => regex_pattern.push_str(MATCH_ONE_SEGMENT),
        GlobToken::Anything => regex_pattern.push_str(MATCH_ANYTHING),
    }
}

fn glob_to_regex(glob: &str) -> Result<Regex> {
    let mut regex_pattern = String::with_capacity(glob.len() * 2);
    regex_pattern.push_str("(?i)^");

    for token in glob_tokens(glob) {
        push_token_regex(&mut regex_pattern, &token);
    }
    regex_pattern.push('$');

    Ok(Regex::new(&regex_pattern)?)
}

/// Builds a regex that matches every character prefix of a path that the glob
/// could match. `accepts_prefix` applies it at a directory boundary.
fn glob_to_prefix_regex(glob: &str) -> Result<Regex> {
    let mut regex_pattern = String::with_capacity(glob.len() * 4);
    regex_pattern.push_str("(?i)^");
    let mut optional_groups = 0;

    for token in glob_tokens(glob) {
        match token {
            GlobToken::OneSegment | GlobToken::Anything => {
                push_token_regex(&mut regex_pattern, &token);
            }
            GlobToken::Literal(_) | GlobToken::OneChar => {
                regex_pattern.push_str("(?:");
                push_token_regex(&mut regex_pattern, &token);
                optional_groups += 1;
            }
        }
    }
    for _ in 0..optional_groups {
        regex_pattern.push_str(")?");
    }
    regex_pattern.push('$');

    Ok(Regex::new(&regex_pattern)?)
}

fn is_regex_meta_character(c: char) -> bool {
    match c {
        '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
        | '#' | '&' | '-' | '~' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(segments: &[&str]) -> String {
        segments.join(PATH_SEPARATOR_STR)
    }

    fn assert_matches(glob: &str, candidate: &str) {
        let path_glob =
            PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(
            path_glob.is_match(candidate),
            "Expected glob '{glob:?}' to match path '{candidate}'"
        );
    }

    fn assert_does_not_match(glob: &str, candidate: &str) {
        let path_glob =
            PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(
            !path_glob.is_match(candidate),
            "Expected glob '{glob:?}' not to match path '{candidate}'"
        );
    }

    fn assert_accepts_prefix(glob: &str, prefix: &str) {
        let glob =
            PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(
            glob.accepts_prefix(prefix),
            "Expected glob '{glob:?}' to accept directory prefix '{prefix}'"
        );
    }

    fn assert_rejects_prefix(glob: &str, prefix: &str) {
        let glob =
            PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(
            !glob.accepts_prefix(prefix),
            "Expected glob '{glob:?}' to reject directory prefix '{prefix}'"
        );
    }

    mod normalization {
        use super::*;

        #[test]
        fn leaves_an_already_normalized_glob_borrowed() {
            let glob = path(&["src", "*.rs"]);
            let normalized = normalize_glob(&glob);

            assert_eq!(normalized, glob);
            assert!(matches!(normalized, Cow::Borrowed(_)));
        }

        #[test]
        fn converts_inverted_and_collapses_repeated_separators() {
            let glob = format!(
                "src{0}{0}nested{1}{1}{1}*.rs",
                INVERTED_PATH_SEPARATOR, PATH_SEPARATOR
            );

            assert_eq!(normalize_glob(&glob), path(&["src", "nested", "*.rs"]));
        }

        #[test]
        fn build_glob_keeps_the_raw_pattern_and_uses_the_normalized_form() {
            let raw = format!("src{0}{0}*.rs", INVERTED_PATH_SEPARATOR);
            let glob = build_glob(&raw).unwrap();

            assert_eq!(glob.raw_pattern, raw);
            assert_eq!(glob.normalized_pattern, path(&["src", "*.rs"]));
            assert_eq!(glob.normalized_segments, ["src", "*.rs"]);
            assert_eq!(glob.to_string(), path(&["src", "*.rs"]));
        }
    }

    mod validation {
        use super::*;

        #[test]
        fn rejects_an_empty_glob() {
            let error = validate_glob("").unwrap_err();

            assert_eq!(error.to_string(), "Glob cannot be empty");
        }

        #[test]
        fn rejects_leading_and_trailing_path_separators() {
            let leading = format!("{PATH_SEPARATOR}src");
            let trailing = format!("src{PATH_SEPARATOR}");

            assert_eq!(
                validate_glob(&leading).unwrap_err().to_string(),
                "Glob cannot start with path separator"
            );
            assert_eq!(
                validate_glob(&trailing).unwrap_err().to_string(),
                "Glob cannot end with path separator"
            );
        }

        #[test]
        fn validation_happens_after_separator_normalization() {
            let leading = format!("{INVERTED_PATH_SEPARATOR}src");
            let trailing = format!("src{INVERTED_PATH_SEPARATOR}");

            assert_eq!(
                PathGlob::new(&leading).unwrap_err().to_string(),
                "Glob cannot start with path separator"
            );
            assert_eq!(
                PathGlob::new(&trailing).unwrap_err().to_string(),
                "Glob cannot end with path separator"
            );
        }

        #[test]
        fn accepts_relative_literal_and_wildcard_globs() {
            assert!(validate_glob("file.txt").is_ok());
            assert!(validate_glob(&path(&["src", "**", "*.rs"])).is_ok());
        }
    }

    mod regex_conversion {
        use super::*;

        #[test]
        fn anchors_the_regex_and_escapes_literal_characters() {
            let glob = path(&["src", "main.rs"]);
            let regex = glob_to_regex(&glob).unwrap();

            assert_eq!(
                regex.as_str(),
                format!(r"(?i)^src{}main\.rs$", PATH_SEPARATOR_REGEX_ESCAPED)
            );
        }

        #[test]
        fn converts_question_single_star_and_double_star() {
            let glob = format!("?{PATH_SEPARATOR}*.rs{PATH_SEPARATOR}**");
            let regex = glob_to_regex(&glob).unwrap();

            assert_eq!(
                regex.as_str(),
                format!(
                    "(?i)^{MATCH_ONE_CHAR}{}{MATCH_ONE_SEGMENT}\\.rs{}{MATCH_ANYTHING}$",
                    PATH_SEPARATOR_REGEX_ESCAPED, PATH_SEPARATOR_REGEX_ESCAPED
                )
            );
        }

        #[test]
        fn treats_three_or_more_consecutive_stars_as_double_star() {
            let double = glob_to_regex("prefix**suffix").unwrap();
            let many = glob_to_regex("prefix*****suffix").unwrap();

            assert_eq!(many.as_str(), double.as_str());
        }

        #[test]
        fn identifies_every_regex_metacharacter_that_must_be_escaped() {
            for character in r"\.+*?()|[]{}^$#&-~".chars() {
                assert!(
                    is_regex_meta_character(character),
                    "Expected {character:?} to be a regex metacharacter"
                );
            }

            for character in ['a', '7', '_', ':', '@', '!'] {
                assert!(!is_regex_meta_character(character));
            }
        }
    }

    mod matching {
        use super::*;

        #[test]
        fn literals_match_the_entire_path_case_insensitively() {
            let glob = path(&["Src", "Main.rs"]);

            assert_matches(&glob, &path(&["src", "main.RS"]));
            assert_does_not_match(&glob, &path(&["other", "main.rs"]));
            assert_does_not_match(&glob, &format!("{}x", path(&["src", "main.rs"])));
            assert_does_not_match(&glob, &format!("x{}", path(&["src", "main.rs"])));
        }

        #[test]
        fn normalized_glob_separators_match_native_paths() {
            let glob = format!("src{INVERTED_PATH_SEPARATOR}*.rs");

            assert_matches(&glob, &path(&["src", "lib.rs"]));
        }

        #[test]
        fn single_star_matches_zero_or_more_characters_in_one_segment() {
            let glob = path(&["src", "*.rs"]);

            assert_matches(&glob, &path(&["src", ".rs"]));
            assert_matches(&glob, &path(&["src", "path_glob.rs"]));
            assert_does_not_match(&glob, &path(&["src", "nested", "lib.rs"]));
        }

        #[test]
        fn double_star_can_cross_path_separators_or_match_nothing() {
            let suffix_glob = path(&["assets", "**"]);

            assert_matches(&suffix_glob, &path(&["assets", "icons", "logo.png"]));
            assert_matches("file**.txt", "file.txt");
        }

        #[test]
        fn question_mark_matches_exactly_one_non_separator_character() {
            let glob = path(&["logs", "?.txt"]);

            assert_matches(&glob, &path(&["logs", "1.txt"]));
            assert_does_not_match(&glob, &path(&["logs", ".txt"]));
            assert_does_not_match(&glob, &path(&["logs", "12.txt"]));
            assert_does_not_match(&glob, &path(&["logs", "nested", ".txt"]));
        }

        #[test]
        fn regex_metacharacters_are_matched_literally() {
            let literal = ".+()|[]{}^$#&-~";

            assert_matches(literal, literal);
            assert_does_not_match(literal, "x+()|[]{}^$#&-~");
        }

        #[test]
        fn unicode_literals_are_supported() {
            let glob = path(&["música", "canção.txt"]);

            assert_matches(&glob, &path(&["música", "canção.txt"]));
            assert_does_not_match(&glob, &path(&["música", "outra.txt"]));
        }
    }

    mod prefix_matching {
        use super::*;

        #[test]
        fn accepts_the_walk_root_and_an_exact_match() {
            let glob = path(&["a", "b"]);

            assert_accepts_prefix(&glob, "");
            assert_accepts_prefix(&glob, &path(&["a", "b"]));
        }

        #[test]
        fn literal_globs_only_accept_branches_that_can_reach_the_match() {
            let glob = path(&["a", "b", "c", "d"]);

            assert_accepts_prefix(&glob, "a");
            assert_accepts_prefix(&glob, &path(&["a", "b"]));
            assert_accepts_prefix(&glob, &path(&["a", "b", "c"]));
            assert_accepts_prefix(&glob, &path(&["a", "b", "c", "d"]));

            assert_rejects_prefix(&glob, "other");
            assert_rejects_prefix(&glob, &path(&["a", "x"]));
            assert_rejects_prefix(&glob, &path(&["a", "b", "cd"]));
            assert_rejects_prefix(&glob, &path(&["a", "b", "c", "d", "e"]));
        }

        #[test]
        fn accepts_the_documented_directory_walk_scenarios() {
            assert_accepts_prefix(&path(&["a", "b", "**"]), &path(&["a", "b", "c", "d"]));
            assert_accepts_prefix(&path(&["a", "b", "c", "d", "**"]), &path(&["a", "b"]));
            assert_accepts_prefix(&path(&["a", "b", "*", "d", "**"]), &path(&["a", "b", "c"]));
        }

        #[test]
        fn single_star_cannot_cross_a_directory_separator() {
            let glob = path(&["a", "*", "d"]);

            assert_accepts_prefix(&glob, "a");
            assert_accepts_prefix(&glob, &path(&["a", "x"]));
            assert_accepts_prefix(&glob, &path(&["a", "x", "d"]));
            assert_rejects_prefix(&glob, &path(&["a", "x", "y"]));
            assert_rejects_prefix(&glob, &path(&["a", "x", "d", "child"]));
        }

        #[test]
        fn a_partial_directory_name_is_not_a_viable_prefix() {
            assert_rejects_prefix("*.rs", "src");
            assert_rejects_prefix(&path(&["a", "source.rs"]), &path(&["a", "source"]));
            assert_accepts_prefix("*.rs", "lib.rs");
        }

        #[test]
        fn double_star_can_consume_arbitrary_directory_depth() {
            let glob = path(&["base", "**", "target"]);

            assert_accepts_prefix(&glob, "base");
            assert_accepts_prefix(&glob, &path(&["base", "one"]));
            assert_accepts_prefix(&glob, &path(&["base", "one", "two", "three"]));
            assert_rejects_prefix(&glob, "other");

            assert_accepts_prefix(
                &path(&["**", "target"]),
                &path(&["any", "directory", "depth"]),
            );
        }

        #[test]
        fn embedded_double_star_can_cross_separators() {
            let glob = format!("archive**{PATH_SEPARATOR}end");

            assert_accepts_prefix(&glob, &path(&["archive", "year", "month"]));
            assert_rejects_prefix(&glob, "unrelated");
        }

        #[test]
        fn double_star_with_a_file_suffix_accepts_ancestor_directories() {
            let glob = PathGlob::new("**.rs").unwrap();
            let prefix = "src";
            let descendant = path(&["src", "main.rs"]);

            assert!(glob.is_match(&descendant));
            assert!(
                glob.accepts_prefix(prefix),
                "Glob '{glob:?}' must accept '{prefix}' because descendant '{descendant}' matches"
            );
        }

        #[test]
        fn embedded_double_star_suffix_can_be_satisfied_by_a_descendant() {
            let pattern = path(&["base", "**target", "end"]);
            let glob = PathGlob::new(&pattern).unwrap();
            let prefix = path(&["base", "branch"]);
            let descendant = path(&["base", "branch", "target", "end"]);

            assert!(glob.is_match(&descendant));
            assert!(
                glob.accepts_prefix(&prefix),
                "Glob '{glob:?}' must accept '{prefix}' because descendant '{descendant}' matches"
            );
        }

        #[test]
        fn question_mark_requires_one_character_in_the_current_segment() {
            let glob = path(&["a", "?", "d"]);

            assert_accepts_prefix(&glob, "a");
            assert_accepts_prefix(&glob, &path(&["a", "x"]));
            assert_rejects_prefix(&glob, &path(&["a", "xy"]));
        }

        #[test]
        fn prefix_checks_are_case_insensitive() {
            let glob = format!("SRC{PATH_SEPARATOR}**{PATH_SEPARATOR}*.RS");
            let prefix = format!("src{0}{0}nested{0}deeper{0}", PATH_SEPARATOR);

            assert_accepts_prefix(&glob, &prefix);
            assert_rejects_prefix(&glob, &format!("{PATH_SEPARATOR}src"));
        }

        #[test]
        fn regex_metacharacters_in_prefixes_remain_literal() {
            let glob = path(&["a+b", "(target).txt"]);

            assert_accepts_prefix(&glob, "a+b");
            assert_rejects_prefix(&glob, "ab");
        }
    }

    mod deserialization {
        use super::*;

        #[test]
        fn deserializes_and_builds_a_working_path_glob() {
            let glob: PathGlob = serde_json::from_str(r#""src/**/*.rs""#).unwrap();

            assert!(glob.is_match(&path(&["src", "nested", "lib.rs"])));
        }

        #[test]
        fn reports_validation_errors_during_deserialization() {
            let result = serde_json::from_str::<PathGlob>("\"\"");

            assert!(result.is_err());
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Glob cannot be empty")
            );
        }
    }
}
