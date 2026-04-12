//! JSON/YAML/TOML operator.

use std::path::Path;
use serde_json::Value;

use super::{Operator, ValidationContext, ExecutionContext, OperatorResult, OperatorDiagnostic, OperatorError, ExtractedValue};
use crate::analyze::SourceRange;

/// JSON/YAML/TOML operator.
#[derive(Debug, Clone)]
pub struct JsonOperator {
    /// Pattern content (e.g., "{ package: { name: $NAME } }")
    pub pattern: String,
    /// Source span
    pub span: SourceRange,
    /// Rule name for diagnostics
    pub rule_name: String,
    /// Capture variables in the pattern
    pub captures: Vec<String>,
}

impl JsonOperator {
    /// Create from AST slot.
    pub fn from_slot(
        slot: &crate::_0_ast::Slot,
        span: SourceRange,
        rule_name: &str,
    ) -> Option<Self> {
        use crate::_0_ast::Tag;
        
        if let crate::_0_ast::Slot::Tagged { tag, body, .. } = slot {
            if !matches!(tag, Tag::Json) {
                return None;
            }
            
            // Extract captures from the pattern
            let captures = extract_captures_from_pattern(body);
            
            Some(Self {
                pattern: body.clone(),
                span,
                rule_name: rule_name.to_string(),
                captures,
            })
        } else {
            None
        }
    }
    
    /// Parse file content to JSON Value.
    fn parse_file(&self, content: &[u8], ext: &str) -> Option<Value> {
        match ext {
            "json" => serde_json::from_slice(content).ok(),
            "yaml" | "yml" => {
                serde_yaml::from_slice::<serde_yaml::Value>(content)
                    .ok()
                    .and_then(|v| serde_json::to_value(v).ok())
            }
            "toml" => {
                std::str::from_utf8(content)
                    .ok()
                    .and_then(|s| toml::from_str::<toml::Value>(s).ok())
                    .and_then(|v| serde_json::to_value(v).ok())
            }
            _ => None,
        }
    }
    
    /// Extract values from parsed JSON using the pattern.
    fn extract_values(&self, json: &Value) -> Vec<String> {
        // This is a simplified extraction - the actual implementation
        // would use the rules engine's walker
        Vec::new()
    }
    
    /// Sample available paths from files for "Did you mean" suggestions.
    fn sample_available_paths(&self, ctx: &ValidationContext) -> Vec<(String, usize)> {
        // Find files that might match
        let glob_pattern = "**/*.json";
        let mut paths_found: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        
        if let Ok(glob) = globset::Glob::new(glob_pattern) {
            let matcher = glob.compile_matcher();
            
            for entry in walkdir::WalkDir::new(ctx.workspace_root)
                .max_depth(5)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    let relative = path.strip_prefix(ctx.workspace_root).unwrap_or(path);
                    if matcher.is_match(relative) {
                        // Sample paths from this file
                        if let Ok(content) = std::fs::read(path) {
                            if let Some(json) = self.parse_file(&content, "json") {
                                self.collect_paths(&json, "", &mut paths_found);
                            }
                        }
                    }
                }
            }
        }
        
        // Sort by frequency and take top 10
        let mut paths: Vec<_> = paths_found.into_iter().collect();
        paths.sort_by(|a, b| b.1.cmp(&a.1));
        paths.truncate(10);
        paths
    }
    
    /// Recursively collect all paths from JSON.
    fn collect_paths(&self, value: &Value, prefix: &str, paths: &mut std::collections::HashMap<String, usize>) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    *paths.entry(path.clone()).or_insert(0) += 1;
                    self.collect_paths(val, &path, paths);
                }
            }
            Value::Array(arr) => {
                for (i, val) in arr.iter().take(5).enumerate() {
                    let path = format!("{}[{}]", prefix, i);
                    *paths.entry(path).or_insert(0) += 1;
                    self.collect_paths(val, prefix, paths);
                }
            }
            _ => {}
        }
    }
}

impl Operator for JsonOperator {
    fn name(&self) -> &'static str {
        "json"
    }
    
    fn span(&self) -> SourceRange {
        self.span.clone()
    }
    
    fn validate(&self, ctx: &ValidationContext) -> Vec<OperatorDiagnostic> {
        let mut diagnostics = Vec::new();
        
        // Sample available paths for suggestions
        let available = self.sample_available_paths(ctx);
        
        // Check if the pattern would match anything
        // (simplified - would need actual pattern matching logic)
        if available.is_empty() {
            diagnostics.push(OperatorDiagnostic {
                error: OperatorError::PathNotFound {
                    attempted: self.pattern.clone(),
                    available: Vec::new(),
                },
                location: self.span.clone(),
                rule_name: self.rule_name.clone(),
                severity: crate::analyze::Severity::Error,
            });
        }
        
        diagnostics
    }
    
    fn execute(&self, ctx: &ExecutionContext) -> OperatorResult {
        // Would use the rules engine to extract values
        OperatorResult {
            values: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
    
    fn pattern(&self) -> String {
        self.pattern.clone()
    }
}

/// Extract capture variable names from a pattern.
fn extract_captures_from_pattern(pattern: &str) -> Vec<String> {
    let mut captures = Vec::new();
    let re = regex::Regex::new(r"\$([A-Z_][A-Z0-9_]*)").unwrap();
    
    for cap in re.captures_iter(pattern) {
        captures.push(cap[1].to_string());
    }
    
    captures
}
