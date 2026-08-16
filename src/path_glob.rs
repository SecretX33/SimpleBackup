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

    pub fn accepts_prefix(&self, prefix: &str) -> bool {
        let first_segment = self.normalized_segments.get(0).expect("Glob must have at least one segment");
        if first_segment == "**" || first_segment.starts_with("*") && self.normalized_segments.len() == 1 {
            // If the glob is just a single * or **, it matches any prefix
            return true;
        }
        self.regex.is_match(prefix)
    }

    pub fn is_match(&self, url: &str) -> bool {
        self.regex.is_match(url)
    }
}

const PATH_SEPARATOR: char = std::path::MAIN_SEPARATOR;
const PATH_SEPARATOR_STR: &str = std::path::MAIN_SEPARATOR_STR;
const PATH_SEPARATOR_REGEX_ESCAPED: &str = if PATH_SEPARATOR == '/' { "/" } else { r"\\/" };
const INVERTED_PATH_SEPARATOR: char = if PATH_SEPARATOR == '/' { '\\' } else { '/' };

fn build_glob(glob: &str) -> Result<PathGlob> {
    let normalized_glob = normalize_glob(glob);
    validate_glob(&normalized_glob)?;

    let regex = glob_to_regex(&normalized_glob)?;
    let segments = normalized_glob.split(PATH_SEPARATOR)
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    Ok(PathGlob {
        raw_pattern: glob.to_string(),
        normalized_pattern: normalized_glob.to_string(),
        normalized_segments: segments,
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
const MATCH_ONE_SEGMENT: &str = if PATH_SEPARATOR == '/' { r"[^/]*?" } else { r"[^\\/]*?" };
const MATCH_ONE_CHAR: &str = if PATH_SEPARATOR == '/' { r"[^/]" } else { r"[^\\/]" };

fn glob_to_regex(glob: &str) -> Result<Regex> {
    let mut regex_pattern = String::with_capacity(glob.len() * 2);
    regex_pattern.push_str("(?i)^");
    let mut index = 0;

    while index < glob.len() {
        let current = glob.chars().nth(index).unwrap();
        let next = glob.chars().nth(index + 1);

        match (current, next) {
            ('?', _) => regex_pattern.push_str(MATCH_ONE_CHAR),
            ('*', Some('*')) => {
                regex_pattern.push_str(MATCH_ANYTHING);
                index += 1;
                while glob.chars().nth(index + 1) == Some('*') {
                    // Consume any leftover *'s
                    index += 1;
                }
            },
            ('*', _) => regex_pattern.push_str(MATCH_ONE_SEGMENT),
            _ => {
                if is_regex_meta_character(current) {
                    regex_pattern.push('\\');
                }
                regex_pattern.push(current);
            }
        }
        index += 1;
    }
    regex_pattern.push('$');

    Ok(Regex::new(&regex_pattern)?)
}

fn is_regex_meta_character(c: char) -> bool {
    match c {
        '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{'
        | '}' | '^' | '$' | '#' | '&' | '-' | '~' => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_matches(glob: &str, url: &str) {
        let g = PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(g.is_match(url), "Expected glob '{glob}' to match URL '{url}'");
    }

    fn assert_no_match(glob: &str, url: &str) {
        let g = PathGlob::new(glob).unwrap_or_else(|e| panic!("Failed to create glob '{glob}': {e}"));
        assert!(!g.is_match(url), "Expected glob '{glob}' NOT to match URL '{url}'");
    }

    fn regex_str(glob: &str) -> String {
        let protocol_index = glob.find(PROTOCOL_SEPARATOR).unwrap();
        glob_to_regex(glob, protocol_index).unwrap().as_str().to_string()
    }

    //// Literal URL matching

    #[test]
    fn literal_exact_match() {
        assert_matches("https://example.com", "https://example.com");
    }

    #[test]
    fn literal_different_domain_no_match() {
        assert_no_match("https://example.com", "https://other.com");
    }

    #[test]
    fn literal_different_protocol_no_match() {
        assert_no_match("https://example.com", "http://example.com");
    }

    #[test]
    fn literal_no_partial_prefix_match() {
        assert_no_match("https://example.com", "https://example.com.evil.com");
    }

    #[test]
    fn literal_no_partial_suffix_match() {
        assert_no_match("https://example.com/path", "https://example.com/pathextra");
    }

    /// Single * wildcard

    #[test]
    fn single_star_matches_subdomain_segment() {
        assert_matches("https://*.example.com", "https://www.example.com");
    }

    #[test]
    fn single_star_does_not_cross_dot() {
        assert_no_match("https://*.example.com", "https://a.b.example.com");
    }

    #[test]
    fn single_star_does_not_cross_slash() {
        assert_no_match("https://example.com/*", "https://example.com/a/b");
    }

    #[test]
    fn single_star_matches_empty() {
        assert_matches("https://*example.com", "https://example.com");
    }

    #[test]
    fn single_star_in_path() {
        assert_matches("https://example.com/*/page", "https://example.com/foo/page");
    }

    #[test]
    fn single_star_path_no_cross_dot() {
        assert_no_match("https://example.com/*", "https://example.com/foo.bar");
    }

    #[test]
    fn single_star_does_not_cross_colon() {
        assert_no_match("https://*.example.com", "https://a:b.example.com");
    }

    /// Double ** wildcard

    #[test]
    fn double_star_crosses_segments() {
        assert_matches("https://example.com/**", "https://example.com/a/b/c");
    }

    #[test]
    fn double_star_crosses_dots() {
        assert_matches("https://**example.com", "https://a.b.c.example.com");
    }

    #[test]
    fn double_star_matches_empty() {
        assert_matches("https://**example.com", "https://example.com");
    }

    #[test]
    fn double_star_deep_path() {
        assert_matches("https://example.com/**/page", "https://example.com/a/b/c/page");
    }

    #[test]
    fn double_star_entire_domain() {
        assert_matches("https://**", "https://anything.goes.here/and/paths");
    }

    #[test]
    fn double_star_in_middle_of_domain() {
        assert_matches("https://www.**", "https://www.example.com/path");
    }

    #[test]
    fn double_star_subdomain_deep() {
        assert_matches("https://**.com", "https://sub.domain.example.com");
    }

    /// Protocol wildcard

    #[test]
    fn double_star_protocol_matches_https() {
        assert_matches("**://example.com", "https://example.com");
    }

    #[test]
    fn double_star_protocol_restricted_to_segment() {
        // ** before :// uses MATCH_ONE_SEGMENT, so it shouldn't cross dots/slashes
        assert_no_match("**://example.com", "https.extra://example.com");
    }

    #[test]
    fn partial_protocol_wildcard() {
        assert_matches("http*://example.com", "https://example.com");
    }

    #[test]
    fn partial_protocol_no_cross_segment() {
        assert_no_match("http*://example.com", "http.x://example.com");
    }

    /// Query parameters

    #[test]
    fn query_literal_match() {
        assert_matches("https://example.com/search?q=test", "https://example.com/search?q=test");
    }

    #[test]
    fn query_literal_no_match() {
        assert_no_match("https://example.com/search?q=test", "https://example.com/search?q=other");
    }

    #[test]
    fn query_star_matches_value() {
        assert_matches("https://example.com/search?q=*", "https://example.com/search?q=anything");
    }

    #[test]
    fn query_star_matches_across_separators() {
        // * in query params uses MATCH_ANYTHING
        assert_matches("https://example.com/search?q=*", "https://example.com/search?q=a&b=c");
    }

    #[test]
    fn query_slash_before_question_optional() {
        // Glob without slash before ? should match URL with slash before ?
        assert_matches("https://example.com/search?q=test", "https://example.com/search/?q=test");
        assert_matches("https://example.com/search/?q=test", "https://example.com/search?q=test");
        assert_matches("https://example.com/search/?q=test", "https://example.com/search/?q=test");
    }

    #[test]
    fn query_path_slashes_stay_literal() {
        // Slashes before the query (not immediately before ?) should be literal
        assert_no_match("https://example.com/a/b?q=1", "https://example.com/ab?q=1");
    }

    #[test]
    fn query_without_slash_before_question() {
        assert_matches("https://example.com/path/?q=1", "https://example.com/path?q=1");
    }

    #[test]
    fn query_slash_optional_domain_only() {
        // Domain with query params: slash before ? is optional
        assert_matches("https://example.com?q=1", "https://example.com/?q=1");
        assert_matches("https://example.com/?q=1", "https://example.com?q=1");
    }

    #[test]
    fn query_slash_optional_without_protocol() {
        // Without-protocol matching also gets optional slash before ?
        assert_matches("https://example.com/path?q=1", "example.com/path/?q=1");
        assert_matches("https://example.com/path/?q=1", "example.com/path?q=1");
    }

    #[test]
    fn query_wildcard_before_question() {
        assert_matches("https://example.com/*?q=*", "https://example.com/search?q=test");
        // * doesn't cross /, so /search/ won't match when * is right before ?
        assert_no_match("https://example.com/*?q=*", "https://example.com/search/?q=test");
    }

    /// Trailing slash

    #[test]
    fn trailing_slash_optional_when_absent() {
        assert_matches("https://example.com", "https://example.com/");
    }

    #[test]
    fn trailing_slash_optional_when_present() {
        assert_matches("https://example.com/", "https://example.com");
    }

    #[test]
    fn trailing_slash_on_path() {
        assert_matches("https://example.com/path", "https://example.com/path/");
    }

    #[test]
    fn slash_before_query_optional_both_directions() {
        // Slash before ? is optional regardless of which side has it
        assert_matches("https://example.com/path?q=1", "https://example.com/path/?q=1");
        assert_matches("https://example.com/path/?q=1", "https://example.com/path?q=1");
    }

    /// Case insensitivity

    #[test]
    fn case_insensitive_domain() {
        assert_matches("https://EXAMPLE.COM", "https://example.com");
    }

    #[test]
    fn case_insensitive_protocol() {
        assert_matches("HTTPS://example.com", "https://example.com");
    }

    #[test]
    fn case_insensitive_path() {
        assert_matches("https://example.com/PATH", "https://example.com/path");
    }

    #[test]
    fn case_insensitive_with_wildcards() {
        assert_matches("https://*.EXAMPLE.COM", "https://www.example.com");
    }

    /// Metacharacter escaping

    #[test]
    fn dot_is_escaped() {
        assert_no_match("https://example.com", "https://exampleXcom");
    }

    #[test]
    fn plus_is_escaped() {
        assert_matches("https://example.com/a+b", "https://example.com/a+b");
        assert_no_match("https://example.com/a+b", "https://example.com/aab");
    }

    #[test]
    fn parens_are_escaped() {
        assert_matches("https://example.com/(page)", "https://example.com/(page)");
    }

    #[test]
    fn brackets_are_escaped() {
        assert_matches("https://example.com/[1]{2}", "https://example.com/[1]{2}");
    }

    /// Error cases

    #[test]
    fn missing_protocol_separator_is_error() {
        assert!(PathGlob::new("example.com").is_err());
    }

    #[test]
    fn error_message_contains_glob() {
        let err = PathGlob::new("example.com").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("example.com"), "Error should contain the glob: {msg}");
        assert!(msg.contains("://"), "Error should mention '://': {msg}");
    }

    #[test]
    fn minimal_valid_glob() {
        assert!(PathGlob::new("a://b").is_ok());
    }

    /// Serde deserialization

    #[test]
    fn serde_valid_glob() {
        let g: PathGlob = serde_json::from_str(r#""https://example.com""#).unwrap();
        assert!(g.is_match("https://example.com"));
    }

    #[test]
    fn serde_invalid_glob_is_error() {
        let result: core::result::Result<PathGlob, _> = serde_json::from_str(r#""no-protocol""#);
        assert!(result.is_err());
    }

    #[test]
    fn serde_inside_struct() {
        #[derive(Deserialize)]
        struct Config {
            pattern: PathGlob,
        }
        let c: Config = serde_json::from_str(r#"{"pattern": "https://*.example.com"}"#).unwrap();
        assert!(c.pattern.is_match("https://www.example.com"));
    }

    /// Without-protocol matching

    #[test]
    fn without_protocol_plain_domain() {
        assert_matches("https://example.com", "example.com");
    }

    #[test]
    fn without_protocol_with_path() {
        assert_matches("https://example.com/path", "example.com/path");
    }

    #[test]
    fn without_protocol_wildcard() {
        assert_matches("https://*.example.com", "www.example.com");
    }

    #[test]
    fn without_protocol_no_match() {
        assert_no_match("https://example.com", "other.com");
    }

    /// Edge cases

    #[test]
    fn port_number() {
        assert_matches("https://localhost:8080", "https://localhost:8080");
    }

    #[test]
    fn different_port_number() {
        assert_no_match("https://localhost:8080", "https://localhost:8081");
    }

    #[test]
    fn multiple_wildcards() {
        assert_matches("https://*.*.com/*", "https://sub.example.com/page");
    }

    #[test]
    fn triple_star_treated_as_double_plus_single() {
        // *** = ** consumed first (MATCH_ANYTHING), then * (MATCH_ONE_SEGMENT)
        assert_matches("https://***example.com", "https://a.b.example.com");
    }

    #[test]
    fn empty_path_after_domain() {
        assert_matches("https://example.com", "https://example.com");
    }

    /// Bug regressions

    #[test]
    fn no_double_dollar_in_regex() {
        let r = regex_str("https://example.com");
        assert!(!r.contains("$$"), "Regex should not contain '$$': {r}");
    }

    #[test]
    fn protocol_slashes_are_mandatory() {
        assert_no_match("https://example.com", "https:example.com");
    }

    #[test]
    fn protocol_single_slash_no_match() {
        assert_no_match("https://example.com", "https:/example.com");
    }

    #[test]
    fn trailing_slash_still_optional_after_fix() {
        assert_matches("https://example.com/path", "https://example.com/path/");
        assert_matches("https://example.com/path/", "https://example.com/path");
    }

    #[test]
    fn path_slashes_are_mandatory() {
        // Internal path slashes must match literally
        assert_no_match("https://example.com/a/b", "https://example.com/ab");
    }

    /// Real-world patterns

    #[test]
    fn google_search() {
        assert_matches(
            "https://www.google.com/search?q=*",
            "https://www.google.com/search?q=rust+programming",
        );
    }

    #[test]
    fn specific_subdomain_pattern() {
        assert_matches("https://docs.**.com/**", "https://docs.example.com/en/latest/guide");
    }

    #[test]
    fn tracking_url_pattern() {
        assert_matches(
            "https://**.tracking.com/**",
            "https://pixel.tracking.com/collect?id=123&event=click",
        );
    }
}