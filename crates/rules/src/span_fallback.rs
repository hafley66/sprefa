//! Span fallback using text search.
//!
//! When serde-based parsing loses source positions, this module provides
//! approximate span recovery by searching for key/value patterns in the source.
//!
//! This is a temporary solution until tree-sitter CST integration provides
//! exact positions for all formats.

use regex::Regex;

/// Approximate span for a value in a structured file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Find the span of a JSON path pointing to a specific value.
///
/// # Arguments
/// * `source` - The JSON source text
/// * `path` - Path components (e.g., ["dependencies", "express"])
/// * `value` - The expected value to find
///
/// # Example
/// ```
/// use sprefa_rules::span_fallback::find_json_path_span;
///
/// let json = r#"{"dependencies": {"express": "4.18.2"}}"#;
/// let span = find_json_path_span(json, &["dependencies", "express"], "4.18.2");
/// assert!(span.is_some());
/// ```
pub fn find_json_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan> {
    if path.is_empty() {
        return None;
    }

    let key = path.last()?;
    let escaped_value = regex::escape(value);
    
    // Pattern: "key": "value" or "key": value or 'key': 'value'
    let patterns = [
        // Double-quoted key, double-quoted value
        format!(r#""{}"\s*:\s*"{}""#, regex::escape(key), escaped_value),
        // Double-quoted key, unquoted/bare value (numbers, booleans, null)
        format!(r#""{}"\s*:\s*{}"#, regex::escape(key), escaped_value),
        // Single-quoted key (non-standard but common)
        format!(r#"'{}'\s*:\s*['"]?{}['"]?"#, regex::escape(key), escaped_value),
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(mat) = re.find(source) {
                // Return just the value span (after the colon)
                if let Some(value_start) = source[mat.start()..mat.end()].find(value) {
                    let abs_start = mat.start() + value_start;
                    return Some(SourceSpan::new(abs_start, abs_start + value.len()));
                }
            }
        }
    }

    None
}

/// Find the span of a YAML path pointing to a specific value.
///
/// YAML is more complex due to multiple syntaxes for the same structure.
pub fn find_yaml_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan> {
    if path.is_empty() {
        return None;
    }

    let key = path.last()?;
    let escaped_value = regex::escape(value);

    // YAML patterns (in order of specificity)
    let patterns = [
        // Inline: key: "value" or key: 'value' or key: value
        format!(r#"{}\s*:\s*["']?{}["']?"#, regex::escape(key), escaped_value),
        // Array item: - value (when key is array index)
        format!(r#"-\s*["']?{}["']?"#, escaped_value),
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(&format!("(?m)^{}$", pattern)) {
            if let Some(mat) = re.find(source) {
                if let Some(value_start) = source[mat.start()..mat.end()].find(value) {
                    let abs_start = mat.start() + value_start;
                    return Some(SourceSpan::new(abs_start, abs_start + value.len()));
                }
            }
        }
    }

    None
}

/// Find the span of a TOML path pointing to a specific value.
pub fn find_toml_path_span(source: &str, path: &[&str], value: &str) -> Option<SourceSpan> {
    if path.is_empty() {
        return None;
    }

    // TOML uses dotted keys for nested access: package.name = "value"
    let dotted_key = path.join(".");
    let escaped_value = regex::escape(value);

    let patterns = [
        // Dotted key: package.name = "value"
        format!(r#"{}\s*=\s*"{}""#, regex::escape(&dotted_key), escaped_value),
        // Dotted key with single quotes
        format!(r#"{}\s*=\s*'{}'"#, regex::escape(&dotted_key), escaped_value),
        // Dotted key with bare value (numbers, booleans)
        format!(r#"{}\s*=\s*{}"#, regex::escape(&dotted_key), escaped_value),
        // Just the last key component (for [table] sections)
        format!(r#"{}\s*=\s*"{}""#, regex::escape(path.last().unwrap()), escaped_value),
        format!(r#"{}\s*=\s*'{}'"#, regex::escape(path.last().unwrap()), escaped_value),
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(&format!("(?m)^{}$", pattern)) {
            if let Some(mat) = re.find(source) {
                if let Some(value_start) = source[mat.start()..mat.end()].find(value) {
                    let abs_start = mat.start() + value_start;
                    return Some(SourceSpan::new(abs_start, abs_start + value.len()));
                }
            }
        }
    }

    None
}

/// Guess the format from file extension and find span accordingly.
pub fn find_path_span(source: &str, path: &[&str], value: &str, extension: &str) -> Option<SourceSpan> {
    match extension.to_lowercase().as_str() {
        "json" | "jsonc" => find_json_path_span(source, path, value),
        "yaml" | "yml" => find_yaml_path_span(source, path, value),
        "toml" => find_toml_path_span(source, path, value),
        _ => {
            // Try all formats
            find_json_path_span(source, path, value)
                .or_else(|| find_yaml_path_span(source, path, value))
                .or_else(|| find_toml_path_span(source, path, value))
        }
    }
}

/// Convert a capture path (from walk) to a search path.
///
/// Walk paths look like: ["dependencies", "express", "version"]
/// We want to find the leaf value, so we use the full path.
pub fn path_from_walk(path: &[String]) -> Vec<&str> {
    path.iter().map(|s| s.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_simple() {
        let json = r#"{"name": "test-package", "version": "1.0.0"}"#;
        
        let span = find_json_path_span(json, &["name"], "test-package");
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!(&json[s.start..s.end], "test-package");
    }

    #[test]
    fn test_json_nested() {
        let json = r#"{"dependencies": {"express": "4.18.2"}}"#;
        
        let span = find_json_path_span(json, &["dependencies", "express"], "4.18.2");
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!(&json[s.start..s.end], "4.18.2");
    }

    #[test]
    fn test_yaml_simple() {
        let yaml = "name: my-service\nversion: 1.0.0\n";
        
        let span = find_yaml_path_span(yaml, &["name"], "my-service");
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!(&yaml[s.start..s.end], "my-service");
    }

    #[test]
    fn test_toml_simple() {
        let toml = r#"
[package]
name = "my-crate"
version = "0.1.0"
"#;
        
        let span = find_toml_path_span(toml, &["package", "name"], "my-crate");
        assert!(span.is_some());
        let s = span.unwrap();
        assert_eq!(&toml[s.start..s.end], "my-crate");
    }

    #[test]
    fn test_toml_dotted_key() {
        let toml = r#"
[package]
name = "my-crate"
"#;
        
        // Can find by just the key name within section context
        let span = find_toml_path_span(toml, &["name"], "my-crate");
        assert!(span.is_some());
    }

    #[test]
    fn test_guess_format_json() {
        let json = r#"{"name": "test"}"#;
        let span = find_path_span(json, &["name"], "test", "json");
        assert!(span.is_some());
    }

    #[test]
    fn test_no_match() {
        let json = r#"{"other": "value"}"#;
        let span = find_json_path_span(json, &["name"], "test");
        assert!(span.is_none());
    }
}
