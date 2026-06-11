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
use crate::{ast, db, lex, parse};

pub fn run_lsp(program_path: &str, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    let mut prog = parse::parse(lex::lex(&src)?)?;
    // Resolve typed path literals (`fs:`/`glob:`) to canonical text before any
    // tick, and keep the TypeDiags: a brand mismatch / escaping literal becomes a
    // squiggle on the `.dl` program file itself (not on a scanned file). These are
    // static, computed once at load, and re-sent on every publish so they survive
    // the per-file republish that clears a scanned file's squiggles.
    let type_diags = crate::typecheck::check_and_normalize(&mut prog, program_path);
    // The program file's own absolute path: TypeDiags point at the `.dl` source,
    // which need not live under the scan root, so they get their own URI.
    let prog_abs = std::fs::canonicalize(program_path)
        .unwrap_or_else(|_| PathBuf::from(program_path));
    let prog_diags = type_diags_to_diagrows(&type_diags, &src);
    // Drop `?` queries: their run_query prints to stdout, which in LSP mode is
    // the protocol channel. `diag` is a derived relation, populated by the
    // fixpoint during tick regardless of any query. We read it via eng.diags().
    prog.items.retain(|i| !matches!(i, crate::ast::Item::Query(_)));
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
        ..Default::default()
    };
    connection.initialize(serde_json::to_value(caps)?)?;

    // Cold tick over the whole tree, then publish every file that has diags.
    eng.tick(&prog, true)?;
    let n = eng.diags(None)?.len();
    eprintln!("[lsp] ready: {n} diagnostic(s) from {} ({} type diag(s))",
        program_path, prog_diags.len());
    // The `.dl` program's own diagnostics (brand/literal type errors) publish once;
    // they do not change between ticks (the program is not re-read).
    publish_program(&connection, &prog_abs, &prog_diags)?;
    publish(&connection, &eng, &root, None)?;

    for msg in &connection.receiver {
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
/// -> its string text -> `module_edge` targets paired by specifier segment
/// (engine `definition_targets`). Result is each target file at 0:0 (a module
/// edge is file-level; the spine carries no in-target symbol position). Null
/// when the cursor is not on a located string or nothing pairs.
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
            .filter_map(|dst| path_to_uri(&root.join(&dst)))
            .map(|uri| Location { uri, range: Range::default() })
            .collect(),
        None => Vec::new(),
    };
    if locations.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    Response::new_ok(req.id.clone(), serde_json::to_value(locations).unwrap_or_default())
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
