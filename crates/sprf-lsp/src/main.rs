use std::collections::HashSet;
use std::sync::Mutex;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use sprefa_sprf::_0_ast::{RuleBody, Statement, Tag};
use sprefa_sprf::_1_parse::parse_program;

mod completion;
mod context;
mod db;
mod hover;
mod workspace;

struct SprfLsp {
    client: Client,
    state: Mutex<DocState>,
    workspace: Mutex<Option<workspace::Workspace>>,
    hover_provider: Mutex<hover::HoverProvider>,
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

    /// Find the rule name that contains the given offset.
    fn rule_at(&self, offset: usize) -> Option<&str> {
        for rule in &self.rules {
            if offset >= rule.span.start && offset <= rule.span.end {
                return Some(&rule.name);
            }
        }
        None
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
    /// Inside a tag body with a specific tag type.
    InsideTag { tag: String, partial: String },
    /// Inside a repo/rev scoped block.
    InsideRepoRev { repo: String, partial: String },
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

        // Check for tag name before the paren
        let tag_start = pre
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let tag_name = &pre[tag_start..];

        // Inside a tag body: fs(...), file(...), folder(...), repo(...), rev(...)
        if !tag_name.is_empty() && Tag::from_str(tag_name).is_some() {
            let paren_content = &before[paren_pos + 1..];
            return CompletionContext::InsideTag {
                tag: tag_name.to_string(),
                partial: paren_content.to_string(),
            };
        }

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

/// Detect context using tokenization (fallback when parsing fails).
fn detect_context_tokenized(text: &str) -> CompletionContext {
    let tokens = context::tokenize(text);
    
    log::debug!("tokenized {} chars into {} tokens", text.len(), tokens.len());
    if log::log_enabled!(log::Level::Debug) {
        for (i, t) in tokens.iter().take(20).enumerate() {
            log::debug!("  token[{}]: {} @ {}-{} ", i, t.token, t.start, t.end);
        }
        if tokens.len() > 20 {
            log::debug!("  ... {} more tokens", tokens.len() - 20);
        }
    }
    
    match context::detect_context(&tokens) {
        context::Context::InsideTag { tag, partial } => {
            log::debug!("detected context: InsideTag(tag='{}', partial='{}')", tag, partial);
            CompletionContext::InsideTag { tag, partial }
        }
        context::Context::InsideRepoRev { repo, partial } => {
            log::debug!("detected context: InsideRepoRev(repo='{}', partial='{}')", repo, partial);
            CompletionContext::InsideRepoRev { repo, partial }
        }
        context::Context::InsideCapture => {
            log::debug!("detected context: InsideCapture");
            CompletionContext::Capture
        }
        context::Context::Unknown => {
            log::debug!("detected context: Unknown");
            CompletionContext::Unknown
        }
    }
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
    (
        "marker",
        "Comment-bounded region scope: marker(\"open\") or marker(\"open\", \"close\")",
    ),
    (
        "md",
        "Markdown structural match: md(## $HEADING), md(- $ITEM), md([$TEXT]($URL)), md(```$LANG)",
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
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        log::info!("LSP initialize request received");
        
        // Use rootUri from initialize params as workspace root
        let root_path = params.root_uri.as_ref().and_then(|uri| {
            uri.to_file_path().ok()
        });
        
        // Load workspace, preferring rootUri location
        let ws = if let Some(root) = root_path {
            // Try loading from specified root first
            let config_path = root.join("sprefa.toml");
            if config_path.exists() {
                workspace::Workspace::from_path(&config_path).ok()
            } else {
                // Create minimal workspace from root
                Some(workspace::Workspace::from_directory(root))
            }
        } else {
            workspace::Workspace::load()
        };
        let db_path = ws.as_ref().and_then(|w| w.db_path.clone());
        
        if let Some(ref workspace) = ws {
            log::info!("workspace loaded: root='{}', repos={}, db_path={}",
                workspace.root.display(),
                workspace.repo_names().len(),
                workspace.db_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "none".to_string())
            );
        } else {
            log::info!("no workspace loaded (not in a sprefa project)");
        }
        
        {
            let mut workspace = self.workspace.lock().unwrap();
            *workspace = ws;
        }
        
        {
            let mut hover_provider = self.hover_provider.lock().unwrap();
            *hover_provider = hover::HoverProvider::new(db_path.as_deref());
        }

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
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        log::info!("LSP initialized");
        self.client
            .log_message(MessageType::INFO, "sprf-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        log::info!("LSP shutdown request received");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        log::info!("document opened: {}", params.text_document.uri);
        self.on_change(&params.text_document.uri, &params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        log::debug!("document changed: {}", params.text_document.uri);
        if let Some(change) = params.content_changes.into_iter().last() {
            self.on_change(&params.text_document.uri, &change.text)
                .await;
        }
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        log::info!("completion request: {} at line {} col {}", uri, pos.line, pos.character);
        
        // Extract all data we need from state before any awaits
        let (text, offset, captures, rule_names, ctx) = {
            let state = self.state.lock().unwrap();
            let offset = position_to_offset(&state.text, pos);
            
            // Try full parse first, fall back to tokenization if it fails
            let ctx = if parse_program(&state.text).is_ok() {
                log::debug!("using parser-based context detection");
                detect_context(&state.text, offset)
            } else {
                // Use tokenization-based context detection
                log::debug!("using tokenization-based context detection (parse failed)");
                detect_context_tokenized(&state.text[..offset])
            };
            
            let captures: Vec<String> = state.captures_at(offset);
            let rule_names: Vec<String> = state.rule_names.clone();
            let text = state.text.clone();
            
            (text, offset, captures, rule_names, ctx)
        };

        let mut items = vec![];

        match &ctx {
            CompletionContext::TagName => {
                log::info!("completion context: TagName, providing {} tags", TAGS.len());
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
                log::info!("completion context: RuleOrTag, {} keywords, {} tags, {} rules", 
                    STATEMENT_KEYWORDS.len(), TAGS.len(), rule_names.len());
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
                for name in &rule_names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("cross-rule reference".into()),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::Capture => {
                log::info!("completion context: Capture, {} captures available", captures.len());
                for cap in &captures {
                    items.push(CompletionItem {
                        label: format!("${}", cap),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("capture".into()),
                        insert_text: Some(cap.clone()),
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
                log::info!("completion context: CrossRefBinding, {} captures available", captures.len());
                // Suggest captures from current rule scope
                for cap in &captures {
                    items.push(CompletionItem {
                        label: format!("${}", cap),
                        kind: Some(CompletionItemKind::VARIABLE),
                        detail: Some("capture".into()),
                        insert_text: Some(format!("${}", cap)),
                        ..Default::default()
                    });
                }
            }
            CompletionContext::InsideTag { tag, partial } => {
                log::info!("completion context: InsideTag(tag='{}', partial='{}')", tag, partial);
                
                // Extract workspace data before await
                let (root, repo_names) = {
                    let ws_guard = self.workspace.lock().unwrap();
                    match *ws_guard {
                        Some(ref ws) => (Some(ws.root.clone()), Some(ws.repo_names())),
                        None => (None, None),
                    }
                };
                
                match tag.as_str() {
                    "fs" | "file" => {
                        // File path completions
                        if let Some(root) = root {
                            log::debug!("completing files in '{}' with partial='{}'", root.display(), partial);
                            let completions = completion::complete_files(&root, partial).await;
                            log::info!("file completion returned {} items", completions.len());
                            items.extend(completions);
                        } else {
                            log::debug!("no workspace root, skipping file completion");
                        }
                    }
                    "folder" => {
                        // Directory completions
                        if let Some(root) = root {
                            log::debug!("completing folders in '{}' with partial='{}'", root.display(), partial);
                            let completions = completion::complete_dirs(&root, partial).await;
                            log::info!("folder completion returned {} items", completions.len());
                            items.extend(completions);
                        } else {
                            log::debug!("no workspace root, skipping folder completion");
                        }
                    }
                    "repo" => {
                        // Repository name completions
                        if let Some(names) = repo_names {
                            log::debug!("completing repo names from {} repos with partial='{}'", names.len(), partial);
                            let completions = completion::complete_repo_names(&names, partial);
                            log::info!("repo completion returned {} items", completions.len());
                            items.extend(completions);
                        } else {
                            log::debug!("no workspace repos, skipping repo completion");
                        }
                    }
                    "rev" | "branch" | "tag" => {
                        // Git ref completions
                        if let Some(root) = root {
                            // Find the repo context
                            let repo = find_repo_context(&text, offset);
                            log::debug!("completing git refs: repo_context={:?}, partial='{}'", repo, partial);
                            
                            // Get the repo path - for now use root
                            let repo_path = root;
                            
                            let completions = completion::complete_git_refs(&repo_path, partial).await;
                            log::info!("git ref completion returned {} items", completions.len());
                            items.extend(completions);
                        } else {
                            log::debug!("no workspace root, skipping git ref completion");
                        }
                    }
                    _ => {
                        log::debug!("no special completion for tag '{}'", tag);
                    }
                }
                
                // Also include capture completions when inside tag bodies that might have captures
                if matches!(tag.as_str(), "json" | "ast" | "line") {
                    log::debug!("adding capture completions for {} tag", tag);
                    for cap in &captures {
                        items.push(CompletionItem {
                            label: format!("${}", cap),
                            kind: Some(CompletionItemKind::VARIABLE),
                            detail: Some("capture".into()),
                            insert_text: Some(cap.clone()),
                            ..Default::default()
                        });
                    }
                }
            }
            CompletionContext::InsideRepoRev { repo, partial } => {
                log::info!("completion context: InsideRepoRev(repo='{}', partial='{}')", repo, partial);
                
                // Extract workspace data before await
                let root = {
                    let ws_guard = self.workspace.lock().unwrap();
                    ws_guard.as_ref().map(|ws| ws.root.clone())
                };
                
                // Similar to "rev" tag but with known repo context
                if let Some(root) = root {
                    let _repo = repo; // repo context available if needed
                    let completions = completion::complete_git_refs(&root, partial).await;
                    log::info!("repo/rev completion returned {} items", completions.len());
                    items.extend(completions);
                } else {
                    log::debug!("no workspace root, skipping repo/rev completion");
                }
            }
            CompletionContext::Unknown => {
                log::info!("completion context: Unknown, providing default completions ({} tags, {} rules)",
                    TAGS.len(), rule_names.len());
                for &(tag, detail) in TAGS {
                    items.push(CompletionItem {
                        label: tag.into(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(detail.into()),
                        ..Default::default()
                    });
                }
                for name in &rule_names {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some("cross-rule reference".into()),
                        ..Default::default()
                    });
                }
            }
        }

        log::info!("completion request completed: {} items total", items.len());
        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;
        log::info!("hover request: {} at line {} col {}", uri, pos.line, pos.character);
        
        let state = self.state.lock().unwrap();
        let offset = position_to_offset(&state.text, pos);
        
        // Find capture at position in AST
        if let Some((var, rule_name)) = find_capture_at_position(&state, offset) {
            log::info!("hover: found capture var='{}' in rule='{}'", var, rule_name);
            
            let hover_provider = self.hover_provider.lock().unwrap();
            if let Some(hover) = hover_provider.hover(&var, &rule_name) {
                log::info!("hover: provider returned result for ${}", var);
                return Ok(Some(hover));
            } else {
                log::info!("hover: provider returned no result for ${} in {}", var, rule_name);
            }
        } else {
            log::debug!("hover: no capture found at offset {}", offset);
        }
        
        Ok(None)
    }
}

/// Find the repo context at a given offset by looking for repo(...) patterns.
fn find_repo_context(text: &str, offset: usize) -> Option<String> {
    let before = &text[..offset.min(text.len())];
    
    // Look for the most recent repo(...) pattern before this position
    let mut depth = 0i32;
    let mut last_repo: Option<String> = None;
    
    for (i, ch) in before.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 {
                    // We've exited the current scope, reset
                    depth = 0;
                    last_repo = None;
                }
            }
            _ => {}
        }
        
        // Look for repo( pattern
        if ch == '(' && i >= 4 {
            let before_paren = &before[..i];
            if before_paren.ends_with("repo") {
                // Extract the repo name from inside the parens
                let after_paren = &before[i + 1..];
                if let Some(close_idx) = after_paren.find(')') {
                    let repo_content = &after_paren[..close_idx];
                    last_repo = Some(repo_content.trim().to_string());
                }
            }
        }
    }
    
    last_repo
}

/// Find capture variable and rule name at a given position.
fn find_capture_at_position(state: &DocState, offset: usize) -> Option<(String, String)> {
    // Find which rule we're in
    let rule_name = state.rule_at(offset)?;
    
    // Look for $VAR pattern around the offset
    let text = &state.text;
    
    // Find the word at the current position
    let before = &text[..offset.min(text.len())];
    let after = &text[offset.min(text.len())..];
    
    // Find start of current word
    let word_start = before
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .map(|i| i + 1)
        .unwrap_or(0);
    
    // Find end of current word
    let word_end = offset + after
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(after.len());
    
    let word = &text[word_start..word_end];
    
    // Check if it's a capture variable (starts with $)
    if word.starts_with('$') {
        let var_name = &word[1..]; // Remove the $
        if !var_name.is_empty() {
            return Some((var_name.to_string(), rule_name.to_string()));
        }
    }
    
    None
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
            log::debug!("parse error in {}: {}", uri, err_msg);
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

        log::debug!("publishing {} diagnostics for {}", diags.len(), uri);
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
    // Initialize env_logger for configurable logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    
    log::info!("sprf-lsp starting up");
    
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| SprfLsp {
        client,
        state: Mutex::new(DocState::default()),
        workspace: Mutex::new(None),
        hover_provider: Mutex::new(hover::HoverProvider::empty()),
    });

    log::info!("LSP service created, starting server");
    Server::new(stdin, stdout, socket).serve(service).await;
    log::info!("sprf-lsp shutting down");
}
