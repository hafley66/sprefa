//! Diagnostic provider for validating fs() patterns match at least one file.

use std::path::Path;
use std::time::Duration;
use tokio::time::timeout;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use log;

use sprefa_sprf::_0_ast::{RuleBody, Slot, Statement, Tag};

/// Pattern that needs validation.
pub struct PatternValidation {
    pub pattern: String,
    pub line: u32,
    pub start_col: u32,
    pub end_col: u32,
}

/// Extract all fs() and folder() patterns from AST.
/// Also takes raw_text to find actual line/col positions.
pub fn extract_patterns(statements: &[Statement], raw_text: &str) -> Vec<PatternValidation> {
    let mut patterns = Vec::new();
    
    log::debug!("extract_patterns: {} statements", statements.len());

    for stmt in statements {
        if let Statement::Rule(rule) = stmt {
            log::debug!("extracting patterns from rule '{}'", rule.name);
            // Flatten each body item
            for body in &rule.body {
                for (_depth, slot) in body.flatten() {
                    if let Some(pattern_str) = extract_pattern_string(&slot) {
                        // Find actual position in raw text
                        if let Some(pos) = find_pattern_position(raw_text, &pattern_str) {
                            log::debug!("found pattern '{}' at line {} col {}", pattern_str, pos.0, pos.1);
                            patterns.push(PatternValidation {
                                pattern: pattern_str,
                                line: pos.0,
                                start_col: pos.1,
                                end_col: pos.1 + pos.2,
                            });
                        }
                    }
                }
            }
        }
    }

    patterns
}

/// Find pattern position in raw text (line, start_col, length)
fn find_pattern_position(text: &str, pattern: &str) -> Option<(u32, u32, u32)> {
    // Find the pattern in text
    if let Some(idx) = text.find(pattern) {
        // Count lines before this index
        let before = &text[..idx];
        let line = before.matches('\n').count() as u32;
        
        // Find column (chars since last newline)
        let last_nl = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = text[last_nl..idx].chars().count() as u32;
        
        let len = pattern.chars().count() as u32;
        
        return Some((line, col, len));
    }
    None
}

fn extract_pattern_string(slot: &Slot) -> Option<String> {
    let (tag, body) = match slot {
        Slot::Bare(body) => {
            log::debug!("bare slot: '{}'", body);
            (Tag::Fs, body)
        }
        Slot::Tagged { tag, body, .. } => {
            log::debug!("tagged slot: {:?} - '{}'", tag, body);
            (*tag, body)
        }
    };

    let pattern = match tag {
        Tag::Fs | Tag::Folder | Tag::File => body,
        _ => return None,
    };

    // Skip patterns with captures (can't validate)
    if pattern.contains('$') {
        return None;
    }

    Some(pattern.clone())
}

/// Validate a pattern matches at least one file.
pub async fn validate_pattern(pattern: &str, root: &Path) -> bool {
    let glob = format!("{}*", pattern);

    let result = timeout(
        Duration::from_millis(200),
        tokio::process::Command::new("rg")
            .args(["--files", "--iglob", &glob, "--max-count", "1"])
            .current_dir(root)
            .output(),
    )
    .await;

    match result {
        Ok(Ok(out)) => {
            // Ripgrep ran successfully
            // Exit code 0 = matches found, exit code 1 = no matches
            if out.status.success() || out.status.code() == Some(1) {
                // Success (0) or no matches (1) - check if we found anything
                !out.stdout.is_empty()
            } else {
                // Other error - skip validation (assume valid)
                true
            }
        }
        _ => true, // Timeout or other error - skip validation
    }
}

/// Generate diagnostics for all unmatched patterns.
pub async fn generate_diagnostics(
    patterns: Vec<PatternValidation>,
    root: &Path,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for pat in patterns {
        if !validate_pattern(&pat.pattern, root).await {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(pat.line, pat.start_col),
                    end: Position::new(pat.line, pat.end_col),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("sprefa".to_string()),
                message: format!("Pattern '{}' matches no files", pat.pattern),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }

    diagnostics
}
