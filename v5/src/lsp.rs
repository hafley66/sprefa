//! LSP server mode: turn a `.dl` program's `diag` relation into live editor
//! diagnostics. Save-driven and disk-truth (see docs/lsp.md). We act on
//! didOpen / didSave only; didChange is ignored because the engine reads the
//! file from disk, and unsaved-buffer support is the RAM-only level of the data
//! model (deferred).

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, GotoDefinitionParams, Location, OneOf, Position,
    PublishDiagnosticsParams, Range, ReferenceParams, ServerCapabilities,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams,
};
use std::collections::HashMap;

use crate::engine::{DiagRow, Engine};
use crate::{ast, db};

use tree_sitter::Parser;

pub fn run_lsp(program: Option<&str>, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let files = crate::resolve_programs(program, &root)?;
    // prepare_paths resolves typed path literals (`fs:`/`glob:`) to canonical
    // text before any tick and keeps the TypeDiags: a brand mismatch / escaping
    // literal becomes a squiggle on the `.dl` program file itself (not on a
    // scanned file). These are static, computed once at load, and re-sent on
    // every publish so they survive the per-file republish that clears a
    // scanned file's squiggles.
    let (mut prog, type_diags, display) = crate::prepare_paths(&files)?;
    // The program file's own absolute path: TypeDiags point at the `.dl` source,
    // which need not live under the scan root, so they get their own URI. Only a
    // single explicit file gets a URI; a discovered `.dl/*.dl` merge has no
    // per-file span attribution, so its TypeDiags render to stderr instead.
    let (prog_abs, prog_diags) = if files.len() == 1 {
        let src = std::fs::read_to_string(&files[0]).unwrap_or_default();
        let abs = std::fs::canonicalize(&files[0]).unwrap_or_else(|_| files[0].clone());
        (Some(abs), type_diags_to_diagrows(&type_diags, &src))
    } else {
        for d in &type_diags {
            eprintln!("{}:1: {}[{}]: {}", d.path, d.severity.as_str(), d.code, d.msg);
        }
        (None, Vec::new())
    };
    // Drop `?` queries: their run_query prints to stdout, which in LSP mode is
    // the protocol channel. `diag` is a derived relation, populated by the
    // fixpoint during tick regardless of any query. We read it via eng.diags().
    // Drop `gen` rules too: a diagnostics tick must never write files.
    prog.items.retain(|i| !matches!(i,
        crate::ast::Item::Query(_) | crate::ast::Item::Gen(_)));
    let conn = db::open(db_path)?;
    let mut eng = Engine::new(conn, root.clone());

    let (connection, io_threads) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
            open_close: Some(true),
            change: Some(TextDocumentSyncKind::NONE),
            save: Some(TextDocumentSyncSaveOptions::Supported(true)),
            ..Default::default()
        })),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        ..Default::default()
    };
    connection.initialize(serde_json::to_value(caps)?)?;

    // Cold tick over the whole tree, then publish every file that has diags.
    eng.tick(&prog, true)?;
    let n = eng.diags(None)?.len();
    eprintln!("[lsp] ready: {n} diagnostic(s) from {} ({} type diag(s))",
        display, type_diags.len());
    // The `.dl` program's own diagnostics (brand/literal type errors) publish once;
    // they do not change between ticks (the program is not re-read).
    if let Some(pa) = &prog_abs {
        publish_program(&connection, pa, &prog_diags)?;
    }
    publish(&connection, &eng, &root, None)?;

    // Daemon subscription (best-effort): when daemon mode is enabled and the
    // daemon on this root is up, subscribe to `diag_changed` so a watcher tick
    // (file save from anywhere, git ref move, config edit) re-publishes live
    // squiggles without waiting for the next editor save. The subscriber thread
    // forwards each push as a synthetic LSP Notification (`dl/diagChanged`)
    // through the connection's sender; the main loop recognizes it and re-
    // publishes from this engine's current view of the shared db.
    if crate::daemon::enabled_for(&root) {
        let root_clone = root.clone();
        let push_sender = connection.sender.clone();
        std::thread::Builder::new().name("dl-lsp-subscriber".into())
            .spawn(move || spawn_daemon_subscriber(root_clone, push_sender))?;
    }

    for msg in &connection.receiver {
        // Synthetic internal notification: daemon pushed diag_changed. The
        // `paths` field carries the watcher's changed paths (absolute); empty
        // means "full tick, paths unknown" — re-publish everything. Otherwise
        // re-publish only the touched files so an unrelated file's squiggles
        // don't flicker.
        if let Message::Notification(ref n) = msg {
            if n.method == "dl/diagChanged" {
                let paths: Vec<String> = n.params.get("paths")
                    .and_then(|p| p.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                if paths.is_empty() {
                    if let Err(e) = publish(&connection, &eng, &root, None) {
                        eprintln!("[lsp] daemon-push republish failed: {e}");
                    }
                } else {
                    for p in paths {
                        let abs = PathBuf::from(&p);
                        if let Err(e) = publish(&connection, &eng, &root, Some(&abs)) {
                            eprintln!("[lsp] daemon-push republish failed for {p}: {e}");
                        }
                    }
                }
                continue;
            }
        }
        match msg {
            Message::Request(req) => {
                match req.method.as_str() {
                    "textDocument/definition" => {
                        let resp = handle_definition(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/references" => {
                        let resp = handle_references(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/hover" => {
                        let resp = handle_hover(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    _ => { if connection.handle_shutdown(&req)? { break; } }
                }
            }
            Message::Notification(not) => {
                let touched: Option<PathBuf> = match not.method.as_str() {
                    "textDocument/didSave" => serde_json::from_value::<DidSaveTextDocumentParams>(not.params)
                        .ok().and_then(|p| uri_to_path(&p.text_document.uri)),
                    "textDocument/didOpen" => serde_json::from_value::<DidOpenTextDocumentParams>(not.params)
                        .ok().and_then(|p| uri_to_path(&p.text_document.uri)),
                    _ => None,
                };
                if let Some(abs) = touched {
                    eng.tick_paths(&prog, std::slice::from_ref(&abs), true)?;
                    // .dl files get tree-sitter parse-error squigglies
                    // alongside the engine's diag-relation rows.
                    if abs.extension().and_then(|e| e.to_str()) == Some("dl") {
                        publish_dl_parse_errors(&connection, &abs)?;
                    }
                    publish(&connection, &eng, &root, Some(&abs))?;
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// Best-effort daemon subscription thread. Tries to attach, send `subscribe`,
/// and read framed notifications forever. Each `diag_changed` is forwarded as
/// a synthetic LSP Notification (`dl/diagChanged`) through the connection's
/// sender; the main loop recognizes the method name and re-publishes. Returns
/// silently on any failure (no daemon = no push; the LSP still works save-driven).
fn spawn_daemon_subscriber(root: PathBuf, sender: crossbeam_channel::Sender<lsp_server::Message>) {
    use crate::{daemon, rpc};
    if !daemon::enabled_for(&root) { return; }
    if !daemon::is_running(&root) {
        let _ = daemon::ensure_daemon(&root, None);
    }
    let mut s = match daemon::connect(&root) { Ok(s) => s, Err(_) => return };
    let req = rpc::Request::new(0, "subscribe",
        serde_json::json!({"events": ["diag_changed"]}));
    if daemon::rpc_call(&mut s, &req).is_err() { return; }
    loop {
        match rpc::read_frame(&mut s) {
            Ok(Some(body)) => {
                // Forward the framed JSON-RPC notification's params through to
                // the LSP main loop. The synthetic method name (`dl/diagChanged`)
                // is what the main loop matches on; the params carry the
                // changed-paths array.
                let v: serde_json::Value = match serde_json::from_str(&body) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
                let _ = sender.send(lsp_server::Message::Notification(lsp_server::Notification {
                    method: "dl/diagChanged".into(),
                    params,
                }));
            }
            _ => return,
        }
    }
}

/// Query `diag` (optionally for one file), group by path, and send one
/// publishDiagnostics per file. The ticked file is always published even with
/// zero rows, so a fixed lint clears its old squiggles.
fn publish(connection: &Connection, eng: &Engine, root: &Path, only_abs: Option<&Path>) -> Result<()> {
    let only_rel: Option<String> = only_abs.and_then(|a| rel_of(root, a));
    let rows = eng.diags(only_rel.as_deref())?;
    let mut by: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    if let Some(r) = &only_rel { by.entry(r.clone()).or_default(); }
    for d in rows { by.entry(d.path.clone()).or_default().push(to_diag(d)); }
    // Extraction type-drops: a file whose rows the file/dir/path checks dropped
    // gets a file-level squiggle. Their `path` is repo-relative like `diag.path`,
    // so they route through the same per-file grouping. Filter to the ticked file
    // when this is a single-file republish so we never resurrect another file's
    // stale drop.
    for d in eng.extraction_drops() {
        if only_rel.as_deref().map_or(true, |r| r == d.path) {
            by.entry(d.path.clone()).or_default().push(to_diag(d.clone()));
        }
    }
    for (path, diagnostics) in by {
        let Some(uri) = path_to_uri(&root.join(&path)) else { continue };
        let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
        connection.sender.send(Message::Notification(Notification {
            method: "textDocument/publishDiagnostics".into(),
            params: serde_json::to_value(params)?,
        }))?;
    }
    Ok(())
}

/// Publish the `.dl` program file's own type diagnostics against its own URI.
/// Always sends (even with zero rows) so a fixed brand error clears. Separate from
/// `publish` because the program file need not live under the scan root.
fn publish_program(connection: &Connection, prog_abs: &Path, diags: &[DiagRow]) -> Result<()> {
    let Some(uri) = path_to_uri(prog_abs) else { return Ok(()); };
    let diagnostics: Vec<Diagnostic> = diags.iter().cloned().map(to_diag).collect();
    let params = PublishDiagnosticsParams { uri, diagnostics, version: None };
    connection.sender.send(Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".into(),
        params: serde_json::to_value(params)?,
    }))?;
    Ok(())
}

/// textDocument/definition over the ref spine: cursor -> innermost located span
/// -> its string text -> engine `definition_targets`. Two paths in the engine:
/// (1) Phase E rule-driven `def_target(name, file, line, kind)` if the program
/// declares it, landing at the real definition line; (2) module-edge fallback
/// (import specifiers) landing at 0:0. Null when the cursor is not on a located
/// string or nothing pairs.
fn handle_definition(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: GotoDefinitionParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let hit = resolve_span(eng, root, &params.text_document_position_params);
    let locations: Vec<Location> = match hit {
        Some((path, _id, text, _lo, _hi)) => eng.definition_targets(&path, &text)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(dst, line)| {
                let uri = path_to_uri(&root.join(&dst))?;
                // line is 1-based from the rels; LSP wants 0-based. Clamp.
                let line0 = (line - 1).max(0) as u32;
                let range = Range::new(Position::new(line0, 0), Position::new(line0, 0));
                Some(Location { uri, range })
            })
            .collect(),
        None => Vec::new(),
    };
    if locations.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    Response::new_ok(req.id.clone(), serde_json::to_value(locations).unwrap_or_default())
}

/// textDocument/hover over the ref spine: cursor -> innermost located span ->
/// its string text -> engine `hover` (auto-synthesizes markdown from
/// type_entity + call_def). Returns a Hover with the span's range and the
/// markdown content, or null when no entity/callable matches the bare name.
fn handle_hover(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: TextDocumentPositionParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let hit = resolve_span(eng, root, &params);
    let Some((path, _id, text, lo, hi)) = hit else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let md = match eng.hover(&path, &text).unwrap_or(None) {
        Some(m) => m,
        None => return Response::new_ok(req.id.clone(), serde_json::Value::Null),
    };
    // Range is the located span so the editor highlights what the hover resolves.
    let content = std::fs::read_to_string(root.join(&path)).unwrap_or_default();
    let range = span_to_range(&content, lo, hi);
    let hover = lsp_types::Hover {
        range: Some(range),
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: md,
        }),
    };
    Response::new_ok(req.id.clone(), serde_json::to_value(hover).unwrap_or_default())
}

/// textDocument/references over the ref spine: cursor -> innermost located span
/// -> every WORK span interning the same `StringId` (engine `string_spans`),
/// i.e. every located occurrence of the exact string, across files. Includes
/// the span under the cursor. Null when the cursor is not on a located string.
fn handle_references(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: ReferenceParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let hit = resolve_span(eng, root, &params.text_document_position);
    let Some((_path, string_id, _text, _lo, _hi)) = hit else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    // Per-file content cache: each span needs its file's bytes for the
    // byte -> position conversion, and one file often holds many spans.
    let mut contents: HashMap<String, String> = HashMap::new();
    let mut locations = Vec::new();
    for (path, lo, hi) in eng.string_spans(&string_id).unwrap_or_default() {
        let content = contents.entry(path.clone()).or_insert_with(|| {
            std::fs::read_to_string(root.join(&path)).unwrap_or_default()
        });
        let Some(uri) = path_to_uri(&root.join(&path)) else { continue };
        locations.push(Location { uri, range: span_to_range(content, lo, hi) });
    }
    if locations.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    Response::new_ok(req.id.clone(), serde_json::to_value(locations).unwrap_or_default())
}

/// (uri, position) -> the innermost located span under the cursor, as
/// (repo-relative path, string_id, text, lo, hi). Reads the file from disk
/// (save-driven: disk is truth, same as extraction) to convert the position to
/// a byte offset.
fn resolve_span(eng: &Engine, root: &Path, pos: &TextDocumentPositionParams)
    -> Option<(String, String, String, u32, u32)>
{
    let abs = uri_to_path(&pos.text_document.uri)?;
    // Canonicalize before stripping: root is canonical (macOS /var -> /private/var)
    // but client URIs need not be.
    let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
    let rel = rel_of(root, &abs)?;
    let content = std::fs::read_to_string(&abs).ok()?;
    let byte = position_to_byte(&content, pos.position)?;
    let (string_id, text, lo, hi) = eng.span_at(&rel, byte).ok().flatten()?;
    Some((rel, string_id, text, lo, hi))
}

/// LSP Position (0-based line, char-ish column) -> byte offset. Column is a
/// char count from the line start, the same UTF-16-ish approximation as
/// `byte_to_line_col`; exact for the ASCII-dominant source this serves.
fn position_to_byte(content: &str, pos: Position) -> Option<u32> {
    let mut line_start = 0usize;
    let mut line = 0u32;
    while line < pos.line {
        line_start += content[line_start..].find('\n')? + 1;
        line += 1;
    }
    let rest = &content[line_start..];
    let line_end = rest.find('\n').unwrap_or(rest.len());
    let col_bytes: usize = rest[..line_end].chars()
        .take(pos.character as usize)
        .map(|c| c.len_utf8())
        .sum();
    Some((line_start + col_bytes) as u32)
}

/// Byte span [lo, hi) in `content` -> an LSP Range via the shared byte -> line
/// resolver (1-based line there, 0-based here).
fn span_to_range(content: &str, lo: u32, hi: u32) -> Range {
    let (sl, sc) = byte_to_line_col(content, lo);
    let (el, ec) = byte_to_line_col(content, hi);
    Range::new(Position::new(sl - 1, sc), Position::new(el - 1, ec))
}

/// Map each lower-time `TypeDiag` (carrying byte offsets into the `.dl` source) to
/// a `DiagRow` with 1-based line/col. A literal-bearing diag has a real span; the
/// `lo` byte resolves to a line via `byte_to_line_col`. A var-level diag (brand
/// unify, structural brand/anchor error) carries span (0,0) by construction and so
/// stays at line 1: it has no single offending literal to point at, only the whole
/// program. The `path` is the `.dl` program file (already set on the TypeDiag).
fn type_diags_to_diagrows(diags: &[ast::TypeDiag], src: &str) -> Vec<DiagRow> {
    diags.iter().map(|d| {
        let (line, col) = if d.span == (0, 0) { (1, 0) } else { byte_to_line_col(src, d.span.0) };
        let (end_line, end_col) = if d.span == (0, 0) { (1, 0) } else { byte_to_line_col(src, d.span.1) };
        DiagRow {
            path: d.path.clone(),
            line: line as i64, col: col as i64,
            end_line: end_line as i64, end_col: end_col as i64,
            severity: d.severity.as_str().to_string(),
            code: d.code.clone(),
            msg: d.msg.clone(),
            hint: None,
        }
    }).collect()
}

/// Resolve a UTF-8 byte offset into the `.dl` source to (1-based line, 0-based
/// UTF-16-ish col). Col is a char count from the line start, which the client
/// clamps to the actual line; good enough for an ASCII-dominant `.dl` source.
/// Clamps a past-end offset to the last position.
fn byte_to_line_col(src: &str, byte: u32) -> (u32, u32) {
    let byte = (byte as usize).min(src.len());
    let mut line = 1u32;
    let mut line_start = 0usize;
    for (i, b) in src.bytes().enumerate() {
        if i >= byte { break; }
        if b == b'\n' { line += 1; line_start = i + 1; }
    }
    let col = src[line_start..byte].chars().count() as u32;
    (line, col)
}

fn to_diag(d: DiagRow) -> Diagnostic {
    let severity = Some(match d.severity.as_str() {
        "error" => DiagnosticSeverity::ERROR,
        "info" => DiagnosticSeverity::INFORMATION,
        "hint" => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::WARNING,
    });
    let line0 = (d.line - 1).max(0) as u32;
    // No column span (col==0 and no explicit end) ⇒ underline the whole line by
    // ending at the start of the next line; the client clamps to line length.
    let range = if d.col == 0 && d.end_col == 0 && d.end_line == d.line {
        Range::new(Position::new(line0, 0), Position::new(line0 + 1, 0))
    } else {
        let end0 = (d.end_line - 1).max(0) as u32;
        Range::new(Position::new(line0, d.col.max(0) as u32),
                   Position::new(end0, d.end_col.max(0) as u32))
    };
    let message = match &d.hint {
        Some(h) => format!("{}\nhint: {h}", d.msg),
        None => d.msg,
    };
    let code = (!d.code.is_empty()).then(|| lsp_types::NumberOrString::String(d.code));
    Diagnostic { range, severity, code, source: Some("dl".into()), message, ..Default::default() }
}

/// abs path -> path relative to root, forward-slashed (matches stored diag.path).
fn rel_of(root: &Path, abs: &Path) -> Option<String> {
    abs.strip_prefix(root).ok().map(|r| r.to_string_lossy().replace('\\', "/"))
}

/// `file:///abs/path` -> PathBuf, percent-decoded.
fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    let s = uri.as_str();
    let rest = s.strip_prefix("file://")?;
    Some(PathBuf::from(percent_decode(rest)))
}

/// abs path -> `file://` Uri, percent-encoding the bytes URIs disallow.
fn path_to_uri(p: &Path) -> Option<Uri> {
    Uri::from_str(&format!("file://{}", percent_encode(&p.to_string_lossy()))).ok()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(byte) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(byte); i += 3; continue;
            }
        }
        out.push(b[i]); i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'/' | b'-' | b'.' | b'_' | b'~' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' =>
                out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ---------- tree-sitter .dl parse-error diagnostics ----------

/// Parse `abs` (a `.dl` file) with tree-sitter-dl, collect ERROR / MISSING
/// nodes, convert to LSP `Diagnostic` rows, and publish to the editor.
fn publish_dl_parse_errors(connection: &Connection, abs: &Path) -> Result<()> {
    let source_text = match std::fs::read_to_string(abs) {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dl::language().into())
        .map_err(|e| anyhow::anyhow!("tree-sitter-dl language: {e}"))?;
    let tree = match parser.parse(&source_text, None) {
        Some(t) => t,
        None => return Ok(()),
    };
    let errors = collect_parse_errors(tree.root_node(), &source_text);
    let diags: Vec<Diagnostic> = errors
        .iter()
        .map(|(start_byte, end_byte, msg)| {
            let (line, col) = byte_to_line_col(&source_text, *start_byte as u32);
            let (end_line, end_col) = byte_to_line_col(&source_text, *end_byte as u32);
            Diagnostic {
                range: Range {
                    start: Position { line, character: col },
                    end: Position { line: end_line, character: end_col },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("dl".to_string()),
                message: msg.clone(),
                ..Default::default()
            }
        })
        .collect();
    let file_uri =
        Uri::from_str(&format!("file://{}", abs.display())).unwrap_or(Uri::from_str("file:///").unwrap());
    let params = PublishDiagnosticsParams {
        uri: file_uri,
        diagnostics: diags,
        version: None,
    };
    let note = Notification {
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::to_value(params)?,
    };
    connection.sender.send(Message::Notification(note))?;
    Ok(())
}

/// Walk the tree-sitter parse tree and collect every ERROR and MISSING node
/// as `(start_byte, end_byte, message)` triples.
fn collect_parse_errors(node: tree_sitter::Node, source_text: &str) -> Vec<(usize, usize, String)> {
    let mut errors = Vec::new();
    let mut cursor = node.walk();
    loop {
        let current_node = cursor.node();
        if current_node.is_error() {
            let (start_byte, end_byte) = (current_node.start_byte(), current_node.end_byte());
            let snippet = source_text
                .get(start_byte..end_byte)
                .unwrap_or("")
                .trim();
            let message = if snippet.is_empty() {
                "unexpected end of input".to_string()
            } else {
                format!("unexpected: `{snippet}`")
            };
            errors.push((start_byte, end_byte, message));
        } else if current_node.is_missing() {
            let kind = current_node.kind();
            let position = current_node.start_byte();
            errors.push((position, position, format!("missing `{kind}`")));
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return errors;
            }
        }
    }
}


