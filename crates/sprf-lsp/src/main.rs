use std::collections::HashSet;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sprefa_sprf::_0_ast::{LinkDecl, Slot, Statement, Tag};
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
struct RuleInfo {
    span: StmtSpan,
    /// Captures defined in json/ast bodies of this rule.
    captures: Vec<String>,
}

#[derive(Default)]
struct DocState {
    /// Full document text for context detection.
    text: String,
    /// All match kinds defined in the file via match() slots.
    match_kinds: HashSet<String>,
    /// All capture names found across the file (for link() suggestions).
    all_captures: HashSet<String>,
    /// Link src/tgt kinds referenced.
    link_kinds: HashSet<String>,
    /// Per-rule capture info with source spans.
    rules: Vec<RuleInfo>,
    /// Diagnostics from validation (not just parse errors).
    diagnostics: Vec<(StmtSpan, String, DiagnosticSeverity)>,
}

impl DocState {
    fn rebuild(&mut self, text: &str) {
        self.text = text.to_string();

        // Try parsing. On error, try adding a semicolon (user may be mid-statement).
        // If both fail, keep previous match_kinds/captures/rules for completions.
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

        self.match_kinds.clear();
        self.all_captures.clear();
        self.link_kinds.clear();
        self.rules.clear();
        self.diagnostics.clear();

        // Build a map of statement byte ranges by finding ; terminators
        let stmt_spans = find_statement_spans(text);

        // Pass 1: collect all names (match kinds, link kinds, query names, captures)
        let mut query_names: HashSet<String> = HashSet::new();
        let mut rule_data: Vec<(StmtSpan, Vec<String>, HashSet<String>)> = vec![];

        for (idx, stmt) in program.iter().enumerate() {
            let span = stmt_spans.get(idx).cloned().unwrap_or(StmtSpan {
                start: 0,
                end: text.len(),
            });

            match stmt {
                Statement::Rule(chain) => {
                    let mut rule_captures = vec![];
                    let mut rule_match_caps = HashSet::new();

                    for slot in &chain.slots {
                        match slot {
                            Slot::Match { capture, kind, .. } => {
                                self.match_kinds.insert(kind.clone());
                                self.all_captures.insert(capture.clone());
                                rule_match_caps.insert(capture.clone());
                            }
                            Slot::Tagged { tag, body, .. } => {
                                if matches!(tag, Tag::Json | Tag::Ast) {
                                    for cap in extract_captures(body) {
                                        self.all_captures.insert(cap.clone());
                                        rule_captures.push(cap);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    self.rules.push(RuleInfo {
                        span: span.clone(),
                        captures: rule_captures.clone(),
                    });
                    rule_data.push((span, rule_captures, rule_match_caps));
                }
                Statement::Link(LinkDecl {
                    src_kind,
                    tgt_kind,
                    kind_name,
                    ..
                }) => {
                    let link_kind = kind_name.clone().unwrap_or_else(|| {
                        format!("{}__{}", src_kind, tgt_kind)
                    });
                    self.link_kinds.insert(link_kind);
                }
                Statement::Query(decl) => {
                    query_names.insert(decl.head.relation.clone());
                }
            }
        }

        // Pass 2: validate references (order-independent)
        for (idx, stmt) in program.iter().enumerate() {
            let span = stmt_spans.get(idx).cloned().unwrap_or(StmtSpan {
                start: 0,
                end: text.len(),
            });

            match stmt {
                Statement::Rule(_) => {
                    if let Some((_, rule_captures, rule_match_caps)) = rule_data.get(
                        program.iter().take(idx + 1)
                            .filter(|s| matches!(s, Statement::Rule(_)))
                            .count() - 1
                    ) {
                        // Validate: match() captures should reference captures from this rule
                        if let Statement::Rule(chain) = stmt {
                            for slot in &chain.slots {
                                if let Slot::Match { capture, .. } = slot {
                                    if !rule_captures.contains(capture) {
                                        self.diagnostics.push((
                                            span.clone(),
                                            format!("${} is not captured by any json() or ast() slot in this rule", capture),
                                            DiagnosticSeverity::WARNING,
                                        ));
                                    }
                                }
                            }
                        }

                        // Validate: captures without match() slot
                        for cap in rule_captures {
                            if !rule_match_caps.contains(cap) {
                                self.diagnostics.push((
                                    span.clone(),
                                    format!("${} is captured but has no match() slot", cap),
                                    DiagnosticSeverity::HINT,
                                ));
                            }
                        }
                    }
                }
                Statement::Link(LinkDecl {
                    src_kind,
                    tgt_kind,
                    ..
                }) => {
                    if !self.match_kinds.contains(src_kind) {
                        self.diagnostics.push((
                            span.clone(),
                            format!("source kind '{}' is not defined by any match() slot", src_kind),
                            DiagnosticSeverity::WARNING,
                        ));
                    }
                    if !self.match_kinds.contains(tgt_kind) {
                        self.diagnostics.push((
                            span.clone(),
                            format!("target kind '{}' is not defined by any match() slot", tgt_kind),
                            DiagnosticSeverity::WARNING,
                        ));
                    }
                }
                Statement::Query(decl) => {
                    for atom in &decl.body {
                        if atom.relation != decl.head.relation
                            && !self.link_kinds.contains(&atom.relation)
                            && !query_names.contains(&atom.relation)
                        {
                            self.diagnostics.push((
                                span.clone(),
                                format!(
                                    "relation '{}' is not a known link kind or query",
                                    atom.relation
                                ),
                                DiagnosticSeverity::WARNING,
                            ));
                        }
                    }
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
    let mut in_paren = 0i32;

    for (i, ch) in text.char_indices() {
        match ch {
            '(' => in_paren += 1,
            ')' => in_paren -= 1,
            ';' if in_paren <= 0 => {
                spans.push(StmtSpan { start, end: i });
                start = i + 1;
            }
            _ => {}
        }
    }
    // Trailing statement without semicolon
    let trailing = text[start..].trim();
    if !trailing.is_empty() && !trailing.starts_with('#') {
        spans.push(StmtSpan { start, end: text.len() });
    }
    spans
}

/// Pull $SCREAMING captures out of a string (json body, ast body, etc).
fn extract_captures(body: &str) -> Vec<String> {
    let mut caps = vec![];
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'_'
                && (i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_alphanumeric())
            {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i > start {
                let name = &body[start..i];
                if name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) {
                    caps.push(name.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    caps
}

/// Detect what completion context the cursor is in.
enum CompletionContext {
    MatchKind,
    LinkKind,
    LinkPredicate,
    TagName,
    Capture,
    Unknown,
}

fn detect_context(text: &str, offset: usize) -> CompletionContext {
    let before = &text[..offset.min(text.len())];

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

        if pre.ends_with("match") {
            let inside = &before[paren_pos + 1..];
            if inside.contains(',') {
                return CompletionContext::MatchKind;
            }
            return CompletionContext::Capture;
        }

        if pre.ends_with("link") {
            let inside = &before[paren_pos + 1..];
            let comma_count = inside.chars().filter(|&c| c == ',').count();
            if comma_count >= 1 {
                return CompletionContext::LinkPredicate;
            }
            return CompletionContext::LinkKind;
        }

        if pre.ends_with("json") || pre.ends_with("ast") {
            return CompletionContext::Capture;
        }
    }

    let trimmed = before.trim_end();
    if trimmed.is_empty()
        || trimmed.ends_with('>')
        || trimmed.ends_with(';')
        || trimmed.ends_with('\n')
    {
        return CompletionContext::TagName;
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

const PREDICATES: &[&str] = &[
    "norm_eq", "norm2_eq", "string_eq", "target_file_eq", "same_repo", "same_file",
    "stem_eq_src", "stem_eq_tgt", "ext_eq_src", "ext_eq_tgt",
    "dir_eq_src", "dir_eq_tgt",
];

const TAGS: &[(&str, &str)] = &[
    ("fs", "File path glob: fs(**/pattern)"),
    ("json", "JSON/YAML/TOML destructuring: json({ key: $CAP })"),
    ("ast", "ast-grep pattern: ast(pattern) or ast[lang](pattern)"),
    ("re", "Regex on file content: re(pattern)"),
    ("repo", "Repository glob: repo(org/*)"),
    ("rev", "Rev glob (branch or tag): rev(main|v*)"),
    ("match", "Map capture to kind: match($CAP, kind)"),
    ("link", "Link declaration: link(src > tgt, predicate)"),
    ("rule", "Name this rule: rule(my_rule_name)"),
    ("query", "Datalog query: query name($A, $C) :- rel($A, $B), name($B, $C);"),
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
                        ",".into(), "(".into(), ">".into(), "$".into(), " ".into(),
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
            CompletionContext::MatchKind | CompletionContext::LinkKind => {
                for kind in &state.match_kinds {
                    items.push(CompletionItem {
                        label: kind.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some("match kind".into()),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::LinkPredicate => {
                for &pred in PREDICATES {
                    items.push(CompletionItem {
                        label: pred.into(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("link predicate".into()),
                        ..Default::default()
                    });
                }
            }
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
            CompletionContext::Capture => {
                // Scoped: only captures from the current rule
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
            CompletionContext::Unknown => {
                for kind in &state.match_kinds {
                    items.push(CompletionItem {
                        label: kind.clone(),
                        kind: Some(CompletionItemKind::ENUM_MEMBER),
                        detail: Some("match kind".into()),
                        ..Default::default()
                    });
                }
                for &(tag, detail) in TAGS {
                    items.push(CompletionItem {
                        label: tag.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
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
            // Try to extract a useful position from the error message
            let err_msg = e.to_string();
            let (start_pos, end_pos) = guess_error_position(text, &err_msg);
            diags.push(Diagnostic {
                range: Range { start: start_pos, end: end_pos },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("sprf".into()),
                message: err_msg,
                ..Default::default()
            });
        }

        // Validation diagnostics (capture mismatches, unknown link kinds)
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
    // Find the last incomplete statement (no terminating ;)
    let mut last_semi = 0;
    let mut in_paren = 0i32;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' => in_paren += 1,
            ')' => in_paren -= 1,
            ';' if in_paren <= 0 => last_semi = i + 1,
            _ => {}
        }
    }

    // The error is likely in the text after the last semicolon
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
