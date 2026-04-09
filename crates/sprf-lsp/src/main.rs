use std::collections::HashSet;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sprefa_sprf::_0_ast::{RuleBody, Statement, Tag};
use sprefa_sprf::_1_parse::parse_program;

struct SprfLsp {
    client: Client,
    state: Mutex<DocState>,
}

/// Byte range of a statement in the source text.
#[derive(Clone)]
struct StmtSpan {
    start: usize,
    end: usize,
}

/// A parsed rule with its captures and byte span.
#[allow(dead_code)]
struct RuleInfo {
    name: String,
    span: StmtSpan,
    /// Captures defined in json/ast/line bodies of this rule.
    captures: Vec<String>,
}

#[derive(Default)]
struct DocState {
    /// Full document text for context detection.
    text: String,
    /// All capture names found across the file.
    all_captures: HashSet<String>,
    /// Per-rule capture info with source spans.
    rules: Vec<RuleInfo>,
    /// All rule names (for cross-ref completions).
    rule_names: Vec<String>,
    /// All check names.
    check_names: Vec<String>,
    /// Diagnostics from validation (not just parse errors).
    diagnostics: Vec<(StmtSpan, String, DiagnosticSeverity)>,
}

impl DocState {
    fn rebuild(&mut self, text: &str) {
        self.text = text.to_string();

        // Try parsing. On error, try adding a semicolon (user may be mid-statement).
        // If both fail, keep previous captures/rules for completions.
        let program = match parse_program(text) {
            Ok(p) => p,
            Err(_) => match parse_program(&format!("{};", text.trim_end())) {
                Ok(p) => p,
                Err(_) => {
                    self.diagnostics.clear();
                    return;
                }
            },
        };

        self.all_captures.clear();
        self.rules.clear();
        self.rule_names.clear();
        self.check_names.clear();
        self.diagnostics.clear();

        // Build a map of statement byte ranges by finding ; terminators
        let stmt_spans = find_statement_spans(text);

        for (idx, stmt) in program.iter().enumerate() {
            let span = stmt_spans.get(idx).cloned().unwrap_or(StmtSpan {
                start: 0,
                end: text.len(),
            });

            match stmt {
                Statement::Rule(decl) => {
                    self.rule_names.push(decl.name.clone());
                    let mut rule_captures = vec![];

                    fn collect_body_captures(
                        body: &RuleBody,
                        all_caps: &mut HashSet<String>,
                        rule_caps: &mut Vec<String>,
                    ) {
                        match body {
                            RuleBody::Step(slot) => {
                                for cap in slot.captures() {
                                    all_caps.insert(cap.clone());
                                    rule_caps.push(cap);
                                }
                            }
                            RuleBody::Block {
                                slot, children, ..
                            } => {
                                for cap in slot.captures() {
                                    all_caps.insert(cap.clone());
                                    rule_caps.push(cap);
                                }
                                for child in children {
                                    collect_body_captures(child, all_caps, rule_caps);
                                }
                            }
                            RuleBody::Ref {
                                cross_ref,
                                children,
                            } => {
                                for binding in &cross_ref.bindings {
                                    all_caps.insert(binding.var.clone());
                                    rule_caps.push(binding.var.clone());
                                }
                                for child in children {
                                    collect_body_captures(child, all_caps, rule_caps);
                                }
                            }
                        }
                    }
                    for body in &decl.body {
                        collect_body_captures(body, &mut self.all_captures, &mut rule_captures);
                    }

                    self.rules.push(RuleInfo {
                        name: decl.name.clone(),
                        span: span.clone(),
                        captures: rule_captures,
                    });
                }
                Statement::Check(decl) => {
                    self.check_names.push(decl.name.clone());
                }
            }
        }
    }

    /// Get captures available at a byte offset (scoped to the enclosing rule).
    fn captures_at(&self, offset: usize) -> Vec<String> {
        for rule in &self.rules {
            if offset >= rule.span.start && offset <= rule.span.end {
                return rule.captures.clone();
            }
        }
        // Fallback: all captures
        self.all_captures.iter().cloned().collect()
    }
}

/// Find byte ranges for each statement by scanning for `;` terminators.
fn find_statement_spans(text: &str) -> Vec<StmtSpan> {
    let mut spans = vec![];
    let mut start = 0;
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;

    for (i, ch) in text.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ';' if brace_depth <= 0 && paren_depth <= 0 => {
                spans.push(StmtSpan { start, end: i });
                start = i + 1;
            }
            _ => {}
        }
    }
    // Trailing statement without semicolon
    let trailing = text[start..].trim();
    if !trailing.is_empty() && !trailing.starts_with('#') {
        spans.push(StmtSpan {
            start,
            end: text.len(),
        });
    }
    spans
}

/// Detect what completion context the cursor is in.
#[allow(dead_code)]
enum CompletionContext {
    /// At a position where a tag keyword is expected (start of slot).
    TagName,
    /// Inside a json/ast/line body where $CAPTURE is expected.
    Capture,
    /// After an identifier + `(` that looks like cross-ref binding position.
    CrossRefBinding,
    /// At a position that could be a cross-ref rule name or tag.
    RuleOrTag,
    Unknown,
}

fn detect_context(text: &str, offset: usize) -> CompletionContext {
    let before = &text[..offset.min(text.len())];

    // Check if we just typed `$` -- capture context
    if before.ends_with('$') {
        return CompletionContext::Capture;
    }

    // Find the innermost unclosed paren
    let mut depth = 0i32;
    let mut last_open = None;
    for (i, b) in before.bytes().enumerate().rev() {
        match b {
            b')' => depth += 1,
            b'(' => {
                if depth == 0 {
                    last_open = Some(i);
                    break;
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    if let Some(paren_pos) = last_open {
        let pre = before[..paren_pos].trim_end();

        // Inside a tag body: json(...), ast(...), line(...)
        if pre.ends_with("json")
            || pre.ends_with("ast")
            || pre.ends_with("line")
            || pre.ends_with(|c: char| c == ']' && pre.contains("ast["))
        {
            return CompletionContext::Capture;
        }

        // Inside something that looks like cross-ref bindings: identifier(col: $VAR, ...)
        let paren_content = &before[paren_pos + 1..];
        if paren_content.contains(':') && paren_content.contains('$') {
            return CompletionContext::CrossRefBinding;
        }

        // After an identifier that could be cross-ref: `name(`
        let word_start = pre
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let word = &pre[word_start..];
        if !word.is_empty() && Tag::from_str(word).is_none() {
            // Inside parens after a non-tag identifier -- cross-ref binding context
            return CompletionContext::CrossRefBinding;
        }
    }

    let trimmed = before.trim_end();
    if trimmed.is_empty()
        || trimmed.ends_with('>')
        || trimmed.ends_with(';')
        || trimmed.ends_with('{')
        || trimmed.ends_with('\n')
    {
        return CompletionContext::RuleOrTag;
    }

    CompletionContext::Unknown
}

fn position_to_offset(text: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    text.len()
}

fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in text.char_indices() {
        if i == offset {
            return Position::new(line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position::new(line, col)
}

const TAGS: &[(&str, &str)] = &[
    ("fs", "File path glob: fs(**/pattern)"),
    (
        "json",
        "JSON/YAML/TOML destructuring: json({ key: $CAP })",
    ),
    (
        "ast",
        "ast-grep pattern: ast(pattern) or ast[lang](pattern)",
    ),
    (
        "line",
        "Line pattern: line($CAP:$TAG) or line(re:pattern)",
    ),
    ("repo", "Repository glob: repo(org/*)"),
    ("rev", "Rev glob (branch or tag): rev(main|v*)"),
    ("branch", "Branch glob (alias for rev): branch(main|develop)"),
    ("tag", "Tag glob (alias for rev): tag(v*)"),
    ("folder", "Folder path glob: folder(src/components/*)"),
    ("file", "File path glob: file(**/README.md)"),
];

const STATEMENT_KEYWORDS: &[(&str, &str)] = &[
    ("rule", "Rule declaration: rule(name) { selectors };"),
    (
        "check",
        "Check declaration: check(name) { SQL query };",
    ),
];

#[tower_lsp::async_trait]
impl LanguageServer for SprfLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ",".into(),
                        "(".into(),
                        ">".into(),
                        "$".into(),
                        " ".into(),
                        "{".into(),
                    ]),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "sprf-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.on_change(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let state = self.state.lock().unwrap();
        let pos = params.text_document_position.position;
        let offset = position_to_offset(&state.text, pos);
        let ctx = detect_context(&state.text, offset);

        let mut items = vec![];

        match ctx {
            CompletionContext::TagName => {
                for &(tag, detail) in TAGS {
                    items.push(CompletionItem {
                        label: tag.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::RuleOrTag => {
                // Statement keywords at top level
                for &(kw, detail) in STATEMENT_KEYWORDS {
                    items.push(CompletionItem {
                        label: kw.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
                        ..Default::default()
                    });
                }
                // Tags
                for &(tag, detail) in TAGS {
                    items.push(CompletionItem {
                        label: tag.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
                        ..Default::default()
                    });
                }
                // Rule names for cross-refs
                for name in &state.rule_names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("cross-rule reference".into()),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::Capture => {
                for cap in state.captures_at(offset) {
                    items.push(CompletionItem {
                        label: format!("${}", cap),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("capture".into()),
                        insert_text: Some(cap),
                        ..Default::default()
                    });
                }
                items.push(CompletionItem {
                    label: "$_".into(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("wildcard (match any)".into()),
                    insert_text: Some("_".into()),
                    ..Default::default()
                });
            }
            CompletionContext::CrossRefBinding => {
                // Suggest captures from current rule scope
                for cap in state.captures_at(offset) {
                    items.push(CompletionItem {
                        label: format!("${}", cap),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("capture".into()),
                        insert_text: Some(format!("${}", cap)),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::Unknown => {
                for &(tag, detail) in TAGS {
                    items.push(CompletionItem {
                        label: tag.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
                        ..Default::default()
                    });
                }
                for name in &state.rule_names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("cross-rule reference".into()),
                        ..Default::default()
                    });
                }
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }
}

impl SprfLsp {
    async fn on_change(&self, uri: &Url, text: &str) {
        let validation_diags;
        {
            let mut state = self.state.lock().unwrap();
            state.rebuild(text);
            validation_diags = state.diagnostics.clone();
        }

        let mut diags: Vec<Diagnostic> = vec![];

        // Parse error diagnostic
        if let Err(e) = parse_program(text) {
            let err_msg = e.to_string();
            let (start_pos, end_pos) = guess_error_position(text, &err_msg);
            diags.push(Diagnostic {
                range: Range {
                    start: start_pos,
                    end: end_pos,
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sprf".into()),
                message: err_msg,
                ..Default::default()
            });
        }

        // Validation diagnostics
        let text_copy = text.to_string();
        for (span, msg, severity) in validation_diags {
            let start = offset_to_position(&text_copy, span.start);
            let end = offset_to_position(&text_copy, span.end);
            diags.push(Diagnostic {
                range: Range { start, end },
                severity: Some(severity),
                source: Some("sprf".into()),
                message: msg,
                ..Default::default()
            });
        }

        self.client
            .publish_diagnostics(uri.clone(), diags, None)
            .await;
    }
}

/// Heuristic: find the last statement boundary before the error and highlight that line.
fn guess_error_position(text: &str, _err_msg: &str) -> (Position, Position) {
    let mut last_semi = 0;
    let mut brace_depth = 0i32;
    let mut paren_depth = 0i32;

    for (i, ch) in text.char_indices() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            ';' if brace_depth <= 0 && paren_depth <= 0 => last_semi = i + 1,
            _ => {}
        }
    }

    let error_region = &text[last_semi..];
    let trimmed_start = last_semi + error_region.len() - error_region.trim_start().len();

    let start = offset_to_position(text, trimmed_start);
    let end = offset_to_position(text, text.len());
    (start, end)
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| SprfLsp {
        client,
        state: Mutex::new(DocState::default()),
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
