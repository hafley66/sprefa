//! LSP server mode: turn a `.dl` program's `diag` relation into live editor
//! diagnostics. Save-driven and disk-truth (see docs/lsp.md). We act on
//! didOpen / didSave only; didChange is ignored because the engine reads the
//! file from disk, and unsaved-buffer support is the RAM-only level of the data
//! model (deferred).

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_server::{Connection, Message, Notification};
use lsp_types::{
    Diagnostic, DiagnosticSeverity, Position, PublishDiagnosticsParams, Range,
    ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, Uri,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams,
};
use std::collections::HashMap;

use crate::engine::{DiagRow, Engine};
use crate::{db, lex, parse};

pub fn run_lsp(program_path: &str, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    let mut prog = parse::parse(lex::lex(&src)?)?;
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
        ..Default::default()
    };
    connection.initialize(serde_json::to_value(caps)?)?;

    // Cold tick over the whole tree, then publish every file that has diags.
    eng.tick(&prog, true)?;
    let n = eng.diags(None)?.len();
    eprintln!("[lsp] ready: {n} diagnostic(s) from {}", program_path);
    publish(&connection, &eng, &root, None)?;

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? { break; }
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
    Diagnostic { range, severity, source: Some("dl".into()), message: d.msg, ..Default::default() }
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
