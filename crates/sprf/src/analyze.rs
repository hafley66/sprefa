//! Unified analysis pipeline: parse + lower + extract with shared error recovery.
//!
//! This module provides a single entry point for all analysis phases, enabling
//! rich LSP features while keeping the LSP layer thin. It follows the "parser as shell"
//! philosophy - the analysis is a pure function from source to results.
//!
//! ## Architecture
//!
//! ```text
//! source → parse_phase → lower_phase → extract_phase (optional)
//!            ↓              ↓              ↓
//!         stmts+errors   rules+symbols  extractions+diagnostics
//! ```
//!
//! Each phase has error recovery, so partial results are always available.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::_0_ast::{Statement, RuleBody, Slot, Tag};
use crate::_1_parse::parse_program_partial;
use crate::_3_lower::{lower_program, lower_rule_decl};
use sprefa_rules::types::Rule;

/// A single extracted value with provenance.
#[derive(Debug, Clone)]
pub struct ExtractedValue {
    /// The captured value
    pub value: String,
    /// Source file path
    pub file_path: String,
    /// Optional line number
    pub line: Option<usize>,
}

/// Result of extracting from a rule.
#[derive(Debug, Clone)]
pub struct RuleExtraction {
    /// Rule name
    pub rule_name: String,
    /// Number of files scanned
    pub files_scanned: usize,
    /// Results: (file_path, var_name, values)
    pub results: Vec<(PathBuf, String, Vec<String>)>,
}

/// Analysis error from any phase.
#[derive(Debug, Clone)]
pub struct AnalysisError {
    /// Which phase produced the error
    pub phase: Phase,
    /// Source location
    pub location: SourceRange,
    /// Error message
    pub message: String,
    /// Severity level
    pub severity: Severity,
}

/// Analysis phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Parse,
    Lower,
    Extract,
}

/// Severity level for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Source range for diagnostics and goto-definition.
#[derive(Clone, Debug, Default)]
pub struct SourceRange {
    /// Byte offset start
    pub start: usize,
    /// Byte offset end
    pub end: usize,
    /// Line number (0-indexed)
    pub line: usize,
    /// Column number (0-indexed)
    pub column: usize,
}

/// Symbol table for LSP goto-definition.
#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    /// Rule name -> definition location
    pub rules: HashMap<String, SymbolLocation>,
    /// (rule_name, capture_name) -> definition location
    pub captures: HashMap<(String, String), SymbolLocation>,
    /// Cross-references: rule_name -> locations where referenced
    pub cross_refs: HashMap<String, Vec<SourceRange>>,
}

/// Location of a symbol in source.
#[derive(Clone, Debug)]
pub struct SymbolLocation {
    /// Source range
    pub range: SourceRange,
    /// Kind of symbol
    pub kind: SymbolKind,
}

/// Kind of symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    RuleDefinition,
    CaptureDefinition,
    RuleReference,
    CaptureReference,
    CrossRuleReference,
}

/// Complete analysis result with all phases.
#[derive(Debug, Clone)]
pub struct PartialAnalysis {
    /// Original source text
    pub source: String,
    /// Parsed statements (may be partial)
    pub stmts: Vec<Statement>,
    /// Lowered rules (may be partial)
    pub rules: Vec<Rule>,
    /// Extraction results (if extraction was run)
    pub extractions: Vec<RuleExtraction>,
    /// Errors from all phases
    pub errors: Vec<AnalysisError>,
    /// Diagnostics from extraction
    pub diagnostics: Vec<AnalysisDiagnostic>,
    /// Symbol table for goto-definition
    pub symbol_table: SymbolTable,
    /// Statement spans for mapping positions
    pub stmt_spans: Vec<SourceRange>,
}

/// Diagnostic from extraction phase.
#[derive(Debug, Clone)]
pub struct AnalysisDiagnostic {
    /// Rule name
    pub rule: String,
    /// Message
    pub message: String,
    /// Severity
    pub severity: Severity,
}

/// Options for analysis.
#[derive(Debug, Clone)]
pub struct AnalyzeOptions {
    /// Whether to run extraction phase
    pub run_extraction: bool,
    /// Workspace root for file operations
    pub workspace_root: PathBuf,
    /// Unsaved documents: uri -> content
    pub documents: HashMap<String, String>,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        Self {
            run_extraction: false,
            workspace_root: PathBuf::from("."),
            documents: HashMap::new(),
        }
    }
}

/// Main entry point - unified analysis.
///
/// Runs all phases with error recovery, returning partial results even on errors.
pub fn analyze_partial(source: &str, options: AnalyzeOptions) -> PartialAnalysis {
    // Phase 1: Parse with error recovery
    let (stmts, parse_errors, stmt_spans) = parse_phase(source);

    // Phase 2: Lower with per-statement error recovery
    let (rules, lower_errors, symbol_table) = lower_phase(&stmts, &stmt_spans, source);

    // Phase 3: Extract with per-rule error recovery (optional)
    let (extractions, extract_diagnostics) = if options.run_extraction {
        extract_phase(&rules, &options)
    } else {
        (vec![], vec![])
    };

    // Combine all errors
    let mut errors = vec![];
    errors.extend(parse_errors);
    errors.extend(lower_errors);

    PartialAnalysis {
        source: source.to_string(),
        stmts,
        rules,
        extractions,
        errors,
        diagnostics: extract_diagnostics,
        symbol_table,
        stmt_spans,
    }
}

/// Phase 1: Parse source into AST with error recovery.
fn parse_phase(source: &str) -> (Vec<Statement>, Vec<AnalysisError>, Vec<SourceRange>) {
    let partial = parse_program_partial(source);

    let mut errors = vec![];

    // Convert parse errors to analysis errors
    for err in &partial.errors {
        errors.push(AnalysisError {
            phase: Phase::Parse,
            location: SourceRange {
                start: err.offset,
                end: err.offset + err.context.len().min(50),
                line: err.line.saturating_sub(1),
                column: 0, // We could compute this more precisely
            },
            message: err.message.clone(),
            severity: Severity::Error,
        });
    }

    // Compute statement spans
    let spans = compute_stmt_spans(source, &partial.stmts);

    (partial.stmts, errors, spans)
}

/// Compute byte spans for each statement by scanning for semicolons.
fn compute_stmt_spans(source: &str, stmts: &[Statement]) -> Vec<SourceRange> {
    let mut spans = vec![];
    let mut start = 0;
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;

    for (i, ch) in source.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ';' if brace_depth <= 0 && paren_depth <= 0 => {
                if spans.len() < stmts.len() {
                    spans.push(SourceRange {
                        start,
                        end: i,
                        line: line_number(source, start),
                        column: column_number(source, start),
                    });
                }
                start = i + 1;
            }
            _ => {}
        }
    }

    // Trailing statement without semicolon
    let trailing = source[start..].trim();
    if !trailing.is_empty() && spans.len() < stmts.len() {
        spans.push(SourceRange {
            start,
            end: source.len(),
            line: line_number(source, start),
            column: column_number(source, start),
        });
    }

    spans
}

/// Phase 2: Lower AST to rules with symbol table construction.
fn lower_phase(
    stmts: &[Statement],
    spans: &[SourceRange],
    source: &str,
) -> (Vec<Rule>, Vec<AnalysisError>, SymbolTable) {
    let mut rules = vec![];
    let mut errors = vec![];
    let mut symbols = SymbolTable::default();

    for (idx, stmt) in stmts.iter().enumerate() {
        let span = spans.get(idx).cloned().unwrap_or_default();

        match stmt {
            Statement::Rule(decl) => {
                match lower_rule_decl(decl) {
                    Ok(rule_vec) => {
                        // Record rule symbol
                        symbols.rules.insert(
                            decl.name.clone(),
                            SymbolLocation {
                                range: span.clone(),
                                kind: SymbolKind::RuleDefinition,
                            },
                        );

                        // Record capture symbols
                        for rule in &rule_vec {
                            for match_def in &rule.create_matches {
                                symbols.captures.insert(
                                    (decl.name.clone(), match_def.capture.clone()),
                                    SymbolLocation {
                                        range: span.clone(),
                                        kind: SymbolKind::CaptureDefinition,
                                    },
                                );
                            }
                        }

                        rules.extend(rule_vec);
                    }
                    Err(e) => {
                        errors.push(AnalysisError {
                            phase: Phase::Lower,
                            location: span,
                            message: format!("Failed to lower rule '{}': {}", decl.name, e),
                            severity: Severity::Error,
                        });
                    }
                }
            }
            Statement::Check(_) => {
                // Check statements don't produce rules but could record symbols if needed
            }
        }
    }

    // Collect cross-references by scanning rule bodies
    collect_cross_refs(stmts, spans, source, &mut symbols);

    (rules, errors, symbols)
}

/// Collect cross-references from rule bodies.
fn collect_cross_refs(
    stmts: &[Statement],
    spans: &[SourceRange],
    _source: &str,
    symbols: &mut SymbolTable,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        let span = spans.get(idx).cloned().unwrap_or_default();

        if let Statement::Rule(decl) = stmt {
            collect_cross_refs_from_body(&decl.body, &decl.name, &span, symbols);
        }
    }
}

/// Recursively collect cross-references from rule bodies.
fn collect_cross_refs_from_body(
    bodies: &[RuleBody],
    rule_name: &str,
    base_span: &SourceRange,
    symbols: &mut SymbolTable,
) {
    for body in bodies {
        match body {
            RuleBody::Ref { cross_ref, children } => {
                // Record the cross-reference
                symbols
                    .cross_refs
                    .entry(cross_ref.rule_name.clone())
                    .or_default()
                    .push(base_span.clone());

                // Recurse into children
                collect_cross_refs_from_body(children, rule_name, base_span, symbols);
            }
            RuleBody::Block { children, .. } => {
                collect_cross_refs_from_body(children, rule_name, base_span, symbols);
            }
            _ => {}
        }
    }
}

/// Phase 3: Extract values from files matching rules.
fn extract_phase(
    rules: &[Rule],
    options: &AnalyzeOptions,
) -> (Vec<RuleExtraction>, Vec<AnalysisDiagnostic>) {
    let mut extractions = vec![];
    let mut diagnostics = vec![];

    for rule in rules {
        // Find matching files
        let files = crate::_4_extract::find_matching_files(rule, &options.workspace_root);

        if files.is_empty() {
            diagnostics.push(AnalysisDiagnostic {
                rule: rule.name.clone(),
                message: format!("Rule '{}' matched 0 files - check file pattern", rule.name),
                severity: Severity::Error,
            });
            continue;
        }

        // Extract from each file
        let mut total_captures = 0;
        let mut file_results = vec![];

        for file_path in files.iter().take(50) {
            let uri = file_path_to_uri(file_path);

            // Get content (unsaved or from disk)
            let content = options
                .documents
                .get(&uri)
                .cloned()
                .or_else(|| std::fs::read_to_string(file_path).ok());

            let Some(content) = content else {
                continue;
            };

            // Extract captures for each variable
            for match_def in &rule.create_matches {
                let var_name = &match_def.capture;
                let values =
                    extract_from_content(rule, var_name, file_path, content.as_bytes());
                total_captures += values.len();
                file_results.push((file_path.clone(), var_name.clone(), values));
            }
        }

        if total_captures == 0 {
            // Check if it's a JSON path issue
            let path_issue = check_json_path_mismatch(rule, &files[0], options);

            if path_issue {
                diagnostics.push(AnalysisDiagnostic {
                    rule: rule.name.clone(),
                    message: format!(
                        "Rule '{}' found {} files but JSON/YAML path doesn't match file structure",
                        rule.name,
                        files.len()
                    ),
                    severity: Severity::Error,
                });
            } else {
                diagnostics.push(AnalysisDiagnostic {
                    rule: rule.name.clone(),
                    message: format!(
                        "Rule '{}' found {} files but extracted 0 captures - check capture variables",
                        rule.name,
                        files.len()
                    ),
                    severity: Severity::Warning,
                });
            }
        }

        extractions.push(RuleExtraction {
            rule_name: rule.name.clone(),
            files_scanned: files.len(),
            results: file_results,
        });
    }

    (extractions, diagnostics)
}

/// Check if JSON/YAML path in rule doesn't match actual file structure.
fn check_json_path_mismatch(rule: &Rule, sample_file: &Path, options: &AnalyzeOptions) -> bool {
    // Get content
    let content = std::fs::read_to_string(sample_file).ok();
    let Some(content) = content else {
        return false;
    };

    // Try to parse based on extension
    let ext = sample_file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let parsed: Option<serde_json::Value> = match ext {
        "json" => serde_json::from_str(&content).ok(),
        "yaml" | "yml" => serde_yaml::from_str::<serde_yaml::Value>(&content)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok()),
        "toml" => toml::from_str::<toml::Value>(&content)
            .ok()
            .and_then(|v| serde_json::to_value(v).ok()),
        _ => None,
    };

    // If file parses but has no captures, it's likely a path mismatch
    parsed.is_some()
}

/// Extract values from content for a rule variable.
fn extract_from_content(
    rule: &Rule,
    var_name: &str,
    file_path: &Path,
    content: &[u8],
) -> Vec<String> {
    crate::_4_extract::extract_from_content(rule, var_name, file_path, content)
        .into_iter()
        .map(|v| v.value)
        .collect()
}

/// Symbol at cursor position.
#[derive(Debug, Clone)]
pub struct SymbolAtCursor {
    /// Symbol name
    pub name: String,
    /// Kind of symbol at cursor
    pub kind: CursorSymbolKind,
}

/// Kind of symbol at cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CursorSymbolKind {
    RuleDefinition,
    RuleReference,
    CaptureDefinition,
    CaptureReference,
}

/// Find symbol at a byte offset in the source.
pub fn find_symbol_at_position(analysis: &PartialAnalysis, offset: usize) -> Option<SymbolAtCursor> {
    let source = &analysis.source;

    // Check if offset is within a rule name definition
    for (name, loc) in &analysis.symbol_table.rules {
        if loc.range.start <= offset && offset <= loc.range.end {
            // Check if we're specifically on the rule name (rough approximation)
            let rule_text = &source[loc.range.start..loc.range.end.min(source.len())];
            if let Some(name_pos) = rule_text.find(name) {
                let name_start = loc.range.start + name_pos;
                let name_end = name_start + name.len();
                if name_start <= offset && offset <= name_end {
                    return Some(SymbolAtCursor {
                        name: name.clone(),
                        kind: CursorSymbolKind::RuleDefinition,
                    });
                }
            }
        }
    }

    // Check for capture variable references ($VAR)
    if offset < source.len() && source[offset..].starts_with('$') {
        // Extract var name
        let after_dollar = &source[offset + 1..];
        let var_end = after_dollar
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(after_dollar.len());
        let var_name = &after_dollar[..var_end];

        // Find which rule we're in
        if let Some(rule_name) = find_rule_at_offset(analysis, offset) {
            return Some(SymbolAtCursor {
                name: format!("{}.{}", rule_name, var_name),
                kind: CursorSymbolKind::CaptureReference,
            });
        }

        // Not in a rule, just return the var
        return Some(SymbolAtCursor {
            name: var_name.to_string(),
            kind: CursorSymbolKind::CaptureReference,
        });
    }

    // Check for cross-rule references (rule_name(...))
    if let Some(rule_name) = find_cross_ref_at_offset(analysis, offset) {
        return Some(SymbolAtCursor {
            name: rule_name,
            kind: CursorSymbolKind::RuleReference,
        });
    }

    None
}

/// Get definition location for a symbol.
pub fn get_definition_location(
    analysis: &PartialAnalysis,
    symbol: &SymbolAtCursor,
) -> Option<SymbolLocation> {
    match symbol.kind {
        CursorSymbolKind::RuleDefinition | CursorSymbolKind::RuleReference => {
            analysis.symbol_table.rules.get(&symbol.name).cloned()
        }
        CursorSymbolKind::CaptureReference => {
            // Parse "rule.var" format
            let parts: Vec<_> = symbol.name.split('.').collect();
            if parts.len() == 2 {
                analysis
                    .symbol_table
                    .captures
                    .get(&(parts[0].to_string(), parts[1].to_string()))
                    .cloned()
            } else {
                // Just a bare var - search all captures
                analysis
                    .symbol_table
                    .captures
                    .values()
                    .next()
                    .cloned()
            }
        }
        _ => None,
    }
}

/// Find which rule contains this offset.
fn find_rule_at_offset(analysis: &PartialAnalysis, offset: usize) -> Option<String> {
    for (idx, stmt) in analysis.stmts.iter().enumerate() {
        if let Statement::Rule(decl) = stmt {
            if let Some(span) = analysis.stmt_spans.get(idx) {
                if span.start <= offset && offset <= span.end {
                    return Some(decl.name.clone());
                }
            }
        }
    }
    None
}

/// Find cross-reference at offset (rule_name followed by paren).
fn find_cross_ref_at_offset(analysis: &PartialAnalysis, offset: usize) -> Option<String> {
    let source = &analysis.source;
    if offset >= source.len() {
        return None;
    }

    // Look backwards for an identifier
    let before = &source[..offset];
    let ident_start = before
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let ident = &before[ident_start..];

    // Look forwards for opening paren
    let after = &source[offset..];
    let next_non_ws = after
        .find(|c: char| !c.is_whitespace())
        .and_then(|i| after.chars().nth(i))?;

    if next_non_ws == '(' && !ident.is_empty() {
        // Check if it's a known rule name
        if analysis.symbol_table.rules.contains_key(ident) {
            return Some(ident.to_string());
        }
    }

    None
}

// Helper functions

fn line_number(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())].lines().count().saturating_sub(1)
}

fn column_number(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .lines()
        .last()
        .map(|l| l.len())
        .unwrap_or(0)
}

fn file_path_to_uri(path: &Path) -> String {
    // Simple file:// URI construction without url crate
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    format!("file://{}", absolute.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple_rule() {
        let source = r#"rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };"#;
        let options = AnalyzeOptions::default();
        let analysis = analyze_partial(source, options);

        assert!(analysis.errors.is_empty(), "Expected no errors: {:?}", analysis.errors);
        assert_eq!(analysis.stmts.len(), 1);
        assert_eq!(analysis.rules.len(), 1);
        assert!(analysis.symbol_table.rules.contains_key("pkg"));
        assert!(analysis.symbol_table.captures.contains_key(&("pkg".to_string(), "NAME".to_string())));
    }

    #[test]
    fn test_analyze_partial_recovery() {
        let source = r#"
rule(valid) { fs(**/*.toml) };
rule(broken { json({ name: $NAME })
rule(also_valid) { fs(**/*.json) };
"#;
        let options = AnalyzeOptions::default();
        let analysis = analyze_partial(source, options);

        // Should have errors for broken rule
        assert!(!analysis.errors.is_empty());
        // But should still have valid rules
        assert_eq!(analysis.stmts.len(), 2);
        assert!(analysis.symbol_table.rules.contains_key("valid"));
        assert!(analysis.symbol_table.rules.contains_key("also_valid"));
    }

    #[test]
    fn test_find_symbol_at_position() {
        let source = r#"rule(pkg) { fs(**/Cargo.toml) > json({ package: { name: $NAME } }) };"#;
        let options = AnalyzeOptions::default();
        let analysis = analyze_partial(source, options);

        // Find the $NAME capture - position right at the '$'
        let name_pos = source.find("$NAME").unwrap();
        let symbol = find_symbol_at_position(&analysis, name_pos);
        assert!(symbol.is_some(), "Expected to find symbol at position {}", name_pos);
        let symbol = symbol.unwrap();
        assert_eq!(symbol.kind, CursorSymbolKind::CaptureReference);
        assert!(symbol.name.contains("NAME"));
    }
}
