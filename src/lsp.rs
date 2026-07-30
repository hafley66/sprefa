//! LSP server mode: turn a `.dl` program's `diag` relation into live editor
//! diagnostics. Save-driven and disk-truth (see docs/lsp.md). We act on
//! didOpen / didSave only; didChange is ignored because the engine reads the
//! file from disk, and unsaved-buffer support is the RAM-only level of the data
//! model (deferred).
//!
//! ## Diagnostic muting (session/db-scoped, NOT a CI gate)
//! The server advertises two `workspace/executeCommand` commands:
//!   * `dl.toggleDiagCode(code)` — flip a diagnostic code in the engine's
//!     `diag_mute` set (insert if absent, delete if present) and immediately
//!     republish open diagnostics with muted codes filtered out.
//!   * `dl.listDiagCodes()` — the distinct codes currently in `diag` plus their
//!     muted state, for the editor quick-pick.
//! The mute filter sits at the publish seam (`publish`), NOT inside
//! `Engine::diags`. This is deliberate: `--check` / `--parse-only` read the
//! `diag` relation directly and are UNAFFECTED by a mute row. Muting is an
//! editor-session affordance stored in the db; it never changes the exit code
//! of a check run or what CI sees.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CallHierarchyServerCapability, Diagnostic, DiagnosticSeverity, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentSymbol, DocumentSymbolParams, GotoDefinitionParams, Location, OneOf, Position,
    PublishDiagnosticsParams, Range, ReferenceParams, ServerCapabilities, SymbolInformation,
    SymbolKind, TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TypeHierarchyItem,
    TypeHierarchyPrepareParams, TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Uri,
    WorkspaceSymbolParams,
};
use std::collections::HashMap;

use crate::engine::{DiagRow, Engine, HierarchyItem};
use crate::{ast, db};

use tree_sitter::Parser;

pub fn run_lsp(
    programs: &[String],
    db_path: Option<&str>,
    db_defaulted: bool,
    root: PathBuf,
    diag_db: Option<PathBuf>,
) -> Result<()> {
    // Stand up the connection and complete the initialize handshake FIRST: the
    // client sends its workspace in `rootUri`, which overrides the process cwd
    // as the scan root. This is how the root reaches an editor-spawned server —
    // there is no `--root` flag. Falls back to the cwd root when the client
    // sends no rootUri (older clients, a bare stdio pipe under test).
    let (connection, io_threads) = Connection::stdio();
    let caps = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::NONE),
                save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        // B2 nearly-free navigation: all three read existing rels, all
        // file-scoped or capped, no engine tick.
        document_highlight_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        document_symbol_provider: Some(OneOf::Left(true)),
        hover_provider: Some(lsp_types::HoverProviderCapability::Simple(true)),
        // B5: call hierarchy has a typed capability field; type hierarchy does
        // not exist on `ServerCapabilities` in this lsp-types version (0.97.0
        // wires the request/param/item types in `type_hierarchy.rs` but never
        // added the server-capability field), so it's spliced into the
        // serialized JSON below instead of fighting the crate's typed gap.
        call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
        execute_command_provider: Some(lsp_types::ExecuteCommandOptions {
            commands: vec!["dl.toggleDiagCode".into(), "dl.listDiagCodes".into()],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut caps_value = serde_json::to_value(caps)?;
    if let Some(obj) = caps_value.as_object_mut() {
        obj.insert(
            "typeHierarchyProvider".to_string(),
            serde_json::Value::Bool(true),
        );
    }
    let init_params = connection.initialize(caps_value)?;
    let root = client_root_uri(&init_params).unwrap_or(root);

    // M5.3 `--diag-db` mode: NO engine boot. The rest of this function (db
    // defaulting, `resolve_programs`/`prepare_paths`, `Engine::new`, the cold
    // tick, and the normal message loop) never runs — diagnostics come
    // entirely from polling `diag_db`'s `diag_v5` view. Additive: this branch
    // returns before any of the engine-boot code below executes, so the
    // default `--lsp` path (no `--diag-db`) is byte-for-byte unchanged.
    if let Some(diag_db_path) = diag_db {
        run_diag_db_mode(&connection, &root, diag_db_path)?;
        drop(connection);
        io_threads.join()?;
        return Ok(());
    }

    // Storage-endgame L2: the defaulted db is the per-root db, keyed off the
    // root's canonical path — and the client's rootUri (above) can override the
    // cwd root the CLI keyed it from. Recompute for the RESOLVED root so an
    // editor-spawned server never opens (and grows) a wrong-key db. An
    // explicit `--db` is untouched.
    let db_path_owned: Option<String> = if db_defaulted {
        let root_db = crate::daemon::root_db_path(&root);
        if let Some(parent) = root_db.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Some(root_db.to_string_lossy().into_owned())
    } else {
        db_path.map(str::to_string)
    };
    let db_path = db_path_owned.as_deref();

    let files = crate::resolve_programs(programs, &root)?;
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
            tracing::warn!(
                "{}:1: {}[{}]: {}",
                d.path,
                d.severity.as_str(),
                d.code,
                d.msg
            );
        }
        (None, Vec::new())
    };
    // Drop `?` queries: their run_query prints to stdout, which in LSP mode is
    // the protocol channel. `diag` is a derived relation, populated by the
    // fixpoint during tick regardless of any query. We read it via eng.diags().
    // Drop `gen` rules too: a diagnostics tick must never write files.
    prog.items
        .retain(|i| !matches!(i, crate::ast::Item::Query(_) | crate::ast::Item::Gen(_)));
    let conn = db::open(db_path)?;
    let mut eng = Engine::new(conn, root.clone());
    // Serve the SAME repo set the daemon does (config `[[repos]]`/`[[org]]`, or a
    // `$SPREFA_CONFIG` a client points us at — e.g. the VS Code extension writing
    // the open workspace folders as repos). Empty config -> `set_repos([])`,
    // which falls back to single-root scanning, so this is a no-op for the common
    // one-repo case. Without it the LSP engine ticks single-root over the SHARED
    // cache.db and clobbers the multi-repo rel_* rows the daemon wrote.
    eng.set_repos(crate::daemon::load_repos_eager());

    // Cold tick over the whole tree, then publish every file that has diags.
    eng.tick(&prog, true)?;
    let n = eng.diags(None)?.len();
    let n_type = type_diags.len();
    let display_name = display;
    tracing::debug!(n, n_type, display = %display_name, "[lsp] ready: {n} diagnostic(s) from {display_name} ({n_type} type diag(s))");
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
    //
    // Gated the same way `run_file`'s daemon routing is: only when this session
    // is on the SHARED db (no explicit `--db`, or the caller took the auto-
    // defaulted per-root `roots/<key>/db.sqlite`). An explicit `--db` is the caller opting into
    // isolation (a test sandbox, a scratch inspection db) — attaching to (or
    // spawning) a background daemon for that root would silently escape the
    // isolation the caller asked for. Without this check every `--lsp` session
    // over a `.dl`-having root tries to auto-spawn a daemon regardless of
    // `--db`, which is how a test sandbox leaks a real `dl daemon start`.
    if crate::daemon::enabled_for(&root) && (db_path.is_none() || db_defaulted) {
        let root_clone = root.clone();
        let push_sender = connection.sender.clone();
        std::thread::Builder::new()
            .name("dl-lsp-subscriber".into())
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
                let paths: Vec<String> = n
                    .params
                    .get("paths")
                    .and_then(|p| p.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                if paths.is_empty() {
                    if let Err(e) = publish(&connection, &eng, &root, None) {
                        tracing::warn!(error = %e, "[lsp] daemon-push republish failed: {e}");
                    }
                } else {
                    for p in paths {
                        let abs = PathBuf::from(&p);
                        if let Err(e) = publish(&connection, &eng, &root, Some(&abs)) {
                            tracing::warn!(path = %p, error = %e, "[lsp] daemon-push republish failed for {p}: {e}");
                        }
                    }
                }
                // A5 push-refresh v1: a graph tick landed, so nudge the flow
                // panel (via the extension) to re-run its preset. Params is an
                // object (not null) so a later version can grow tick/moved-rel
                // fields without a wire break.
                let _ = send_graph_changed(&connection);
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
                    "dl/refs" => {
                        let resp = handle_refs(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "dl/locate" => {
                        let resp = handle_locate(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/hover" => {
                        let resp = handle_hover(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/documentHighlight" => {
                        let resp = handle_document_highlight(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/documentSymbol" => {
                        let resp = handle_document_symbol(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "workspace/symbol" => {
                        let resp = handle_workspace_symbol(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/prepareCallHierarchy" => {
                        let resp = handle_call_hierarchy_prepare(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "callHierarchy/incomingCalls" => {
                        let resp = handle_call_hierarchy_incoming(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "callHierarchy/outgoingCalls" => {
                        let resp = handle_call_hierarchy_outgoing(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "textDocument/prepareTypeHierarchy" => {
                        let resp = handle_type_hierarchy_prepare(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "typeHierarchy/supertypes" => {
                        let resp = handle_type_hierarchy_supertypes(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "typeHierarchy/subtypes" => {
                        let resp = handle_type_hierarchy_subtypes(&eng, &root, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "dl/query" => {
                        let resp = handle_query(&eng, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "dl/hookEvent" => {
                        let resp = handle_hook_event(&mut eng, &prog, &req);
                        connection.sender.send(Message::Response(resp))?;
                    }
                    "workspace/executeCommand" => {
                        // Toggle mutates the mute set and republishes; list is
                        // read-only. Both answer through the same request id.
                        let resp = handle_execute_command(&mut eng, &req);
                        connection.sender.send(Message::Response(resp))?;
                        if is_toggle_command(&req) {
                            publish(&connection, &eng, &root, None)?;
                        }
                    }
                    _ => {
                        if connection.handle_shutdown(&req)? {
                            break;
                        }
                    }
                }
            }
            Message::Notification(not) => {
                let touched: Option<PathBuf> = match not.method.as_str() {
                    "textDocument/didSave" => {
                        serde_json::from_value::<DidSaveTextDocumentParams>(not.params)
                            .ok()
                            .and_then(|p| uri_to_path(&p.text_document.uri))
                    }
                    "textDocument/didOpen" => {
                        serde_json::from_value::<DidOpenTextDocumentParams>(not.params)
                            .ok()
                            .and_then(|p| uri_to_path(&p.text_document.uri))
                    }
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
                    // A5: the in-process didSave tick also moved the graph.
                    let _ = send_graph_changed(&connection);
                }
            }
            Message::Response(_) => {}
        }
    }
    drop(connection);
    io_threads.join()?;
    Ok(())
}

/// The workspace root the client declared in `initialize` — `rootUri`, else the
/// first `workspaceFolders` entry. `None` when the client sent neither (the
/// caller keeps the process cwd). Converts a `file://` URI to a local path.
fn client_root_uri(init_params: &serde_json::Value) -> Option<PathBuf> {
    // Canonicalize to match the engine's path keys (macOS temp dirs are
    // symlinks: `/var` -> `/private/var`); the cwd fallback is canonicalized too.
    let to_path = |uri: &str| {
        let rest = uri.strip_prefix("file://")?;
        let p = PathBuf::from(percent_decode(rest));
        Some(p.canonicalize().unwrap_or(p))
    };
    if let Some(p) = init_params
        .get("rootUri")
        .and_then(|v| v.as_str())
        .and_then(to_path)
    {
        return Some(p);
    }
    init_params
        .get("workspaceFolders")
        .and_then(|v| v.as_array())
        .and_then(|folders| folders.first())
        .and_then(|f| f.get("uri"))
        .and_then(|v| v.as_str())
        .and_then(to_path)
}

/// Best-effort daemon subscription thread. Tries to attach and stream the
/// daemon's push notifications from `GET /watch` (SSE over the UDS socket).
/// Each pushed notification is forwarded as a synthetic LSP Notification
/// (`dl/diagChanged`) through the connection's sender; the main loop recognizes
/// the method name and re-publishes. Returns silently on any failure (no
/// daemon = no push; the LSP still works save-driven).
fn spawn_daemon_subscriber(root: PathBuf, sender: crossbeam_channel::Sender<lsp_server::Message>) {
    use crate::daemon;
    if !daemon::enabled_for(&root) {
        return;
    }
    if !daemon::is_running() {
        let _ = daemon::ensure_singleton();
    }
    // Register this workspace root with the singleton so its engine ticks + pushes.
    let _ = daemon::add_root(&root);
    let client = match daemon::connect() {
        Ok(c) => c,
        Err(_) => return,
    };
    // The push stream is process-wide; each notification's params carry a
    // `root` field so the client can tell which engine moved.
    let _ = client.watch(|note| {
        // Forward the JSON-RPC notification's params through to the LSP main
        // loop. The synthetic method name (`dl/diagChanged`) is what the main
        // loop matches on; the params carry the changed-paths array.
        let params = note
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        sender
            .send(lsp_server::Message::Notification(
                lsp_server::Notification {
                    method: "dl/diagChanged".into(),
                    params,
                },
            ))
            .is_ok()
    });
}

// ---------- M5.3: `--diag-db` mode (no engine boot) ----------
//
// The whole mode is: a background thread polls a sqlite file's `diag_v5`
// view on a 500ms cadence and forwards `textDocument/publishDiagnostics`
// notifications through the same `connection.sender` channel the daemon-push
// subscriber above uses; the main thread just runs the connection's receiver
// loop so `shutdown`/`exit` still work. No `Engine`, no tick, no db write —
// the node-side v6 runtime owns `diag_v5` entirely; this reader treats its 9
// columns as the whole interface.

/// One row of the `diag_v5` view: `path, line, col, end_line, end_col,
/// severity, code, msg, hint`. Positions are already 0-based per the view's
/// contract (unlike the engine's `diag` relation, which is 1-based and goes
/// through `to_diag`'s line-1 adjustment).
struct DiagV5Row {
    path: String,
    line: i64,
    col: i64,
    end_line: i64,
    end_col: i64,
    severity: String,
    code: String,
    msg: String,
    hint: Option<String>,
}

/// Run the `--diag-db` message loop: spawn the poll thread, then block on the
/// connection's receiver only long enough to answer `shutdown`/`exit` (no
/// other request is meaningful in this mode — diagnostics arrive from the
/// poll thread, not from editor events). Returns when the client shuts down.
fn run_diag_db_mode(connection: &Connection, root: &Path, diag_db_path: PathBuf) -> Result<()> {
    // Relative paths in `diag_v5.path` resolve against the LSP process's own
    // cwd (the pinned contract), falling back to the resolved workspace root
    // if the cwd is unreadable for some reason.
    let cwd = std::env::current_dir().unwrap_or_else(|_| root.to_path_buf());
    let sender = connection.sender.clone();
    std::thread::Builder::new()
        .name("dl-lsp-diag-db-poll".into())
        .spawn(move || diag_db_poll_loop(diag_db_path, cwd, sender))?;

    for msg in &connection.receiver {
        if let Message::Request(req) = msg {
            if connection.handle_shutdown(&req)? {
                break;
            }
        }
    }
    Ok(())
}

/// Background poll loop for `--diag-db`: every 500ms, check `PRAGMA
/// data_version` on a PERSISTENT read-only connection to `diag_db_path`. On a
/// change from the last observed value, SELECT the 9 `diag_v5` columns, group
/// by path, and publish one `textDocument/publishDiagnostics` per path —
/// INCLUDING an empty-diagnostics publish for any path that was published on
/// a previous round but is absent from the current SELECT (the retraction
/// half: it clears that file's squiggles).
///
/// `data_version` MUST be read repeatedly on the SAME connection to mean
/// anything: verified empirically (sqlite3 CLI + python, both journal and WAL
/// mode) that a brand-new connection's first `PRAGMA data_version` always
/// reads as an unchanging per-connection baseline regardless of how many
/// commits other connections have made — only a SECOND-and-later read on
/// that SAME connection reflects a since-elapsed external commit. Re-opening
/// fresh every poll (the first draft of this loop) would silently never
/// detect a single change. So the connection is held across iterations and
/// only replaced on error (including "the file does not exist yet", the
/// documented startup race) — that reopen-on-error path is `open_diag_db_conn`.
///
/// Never exits on an IO/SQL error or a missing db/view: those log via
/// `tracing` and the loop keeps polling. Ends only when the connection's
/// sender is closed (the LSP session shut down).
fn diag_db_poll_loop(
    diag_db_path: PathBuf,
    cwd: PathBuf,
    sender: crossbeam_channel::Sender<Message>,
) {
    let mut conn: Option<rusqlite::Connection> = None;
    let mut last_data_version: Option<i64> = None;
    let mut last_published_paths: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    loop {
        if conn.is_none() {
            match open_diag_db_conn(&diag_db_path) {
                Ok(opened) => conn = Some(opened),
                Err(e) => {
                    tracing::debug!(
                        error = %e, path = %diag_db_path.display(),
                        "[lsp diag-db] open failed (db may not exist yet), will retry in 500ms"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
            }
        }
        match poll_diag_v5_once(conn.as_ref().unwrap(), &mut last_data_version) {
            Ok(Some(rows)) => {
                let by_path = group_diag_v5_rows(rows);
                let current_paths: std::collections::HashSet<String> =
                    by_path.keys().cloned().collect();
                let mut send_failed = false;
                for (path, diagnostics) in by_path {
                    if publish_diag_v5_path(&sender, &cwd, &path, diagnostics).is_err() {
                        send_failed = true;
                        break;
                    }
                }
                if !send_failed {
                    for stale_path in last_published_paths.difference(&current_paths) {
                        if publish_diag_v5_path(&sender, &cwd, stale_path, Vec::new()).is_err() {
                            send_failed = true;
                            break;
                        }
                    }
                }
                if send_failed {
                    return; // connection closed; nothing left to publish to.
                }
                last_published_paths = current_paths;
            }
            Ok(None) => {} // data_version unchanged since the last poll; nothing to do.
            Err(e) => {
                tracing::warn!(
                    error = %e, path = %diag_db_path.display(),
                    "[lsp diag-db] poll failed, reopening connection and retrying in 500ms"
                );
                // The connection itself may be the broken part (the file was
                // replaced/unlinked under us) — drop it so the next iteration
                // opens fresh via `open_diag_db_conn`, restarting the
                // data_version baseline against whatever is there now.
                conn = None;
                last_data_version = None;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

/// Open `diag_db_path` read-only. Fails (rather than creating the file) when
/// the db does not exist yet — the caller retries on the next poll tick,
/// which is exactly the "db may not exist yet at startup" contract.
fn open_diag_db_conn(diag_db_path: &Path) -> Result<rusqlite::Connection> {
    Ok(rusqlite::Connection::open_with_flags(
        diag_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?)
}

/// One poll cycle against the PERSISTENT `conn` (see `diag_db_poll_loop`'s doc
/// for why this must be the same connection across calls): read `PRAGMA
/// data_version`. `Ok(None)` when the value is unchanged from
/// `*last_data_version` (nothing to re-publish); `Ok(Some(rows))` on a
/// change, having updated `*last_data_version`. A missing `diag_v5` view
/// reads as zero rows (the node-side runtime may not have created it yet);
/// any other sqlite/IO error propagates to the caller, which logs it, drops
/// the connection, and retries next cycle.
fn poll_diag_v5_once(
    conn: &rusqlite::Connection,
    last_data_version: &mut Option<i64>,
) -> Result<Option<Vec<DiagV5Row>>> {
    let data_version: i64 = conn.query_row("PRAGMA data_version", [], |row| row.get(0))?;
    if *last_data_version == Some(data_version) {
        return Ok(None);
    }
    *last_data_version = Some(data_version);

    let query_result = conn
        .prepare(
            "SELECT path, line, col, end_line, end_col, severity, code, msg, hint FROM diag_v5",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok(DiagV5Row {
                    path: row.get(0)?,
                    line: row.get(1)?,
                    col: row.get(2)?,
                    end_line: row.get(3)?,
                    end_col: row.get(4)?,
                    severity: row.get(5)?,
                    code: row.get(6)?,
                    msg: row.get(7)?,
                    hint: row.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        });
    let rows = match query_result {
        Ok(rows) => rows,
        // The view isn't there yet (node-side runtime hasn't created it, or
        // this poll raced a DROP/recreate) — treat exactly like zero rows,
        // per the pinned mode contract, rather than surfacing an error.
        Err(rusqlite::Error::SqliteFailure(_, Some(msg))) if msg.contains("no such table") => {
            Vec::new()
        }
        Err(e) => return Err(e.into()),
    };
    Ok(Some(rows))
}

/// Group `diag_v5` rows by `path`, converting each to an LSP `Diagnostic`.
fn group_diag_v5_rows(rows: Vec<DiagV5Row>) -> HashMap<String, Vec<Diagnostic>> {
    let mut by_path: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for row in rows {
        by_path
            .entry(row.path.clone())
            .or_default()
            .push(diag_v5_to_diagnostic(row));
    }
    by_path
}

/// `diag_v5.severity` string -> LSP `DiagnosticSeverity`. Per the pinned
/// mapping: error/warn/info/hint -> ERROR/WARNING/INFORMATION/HINT; any other
/// string (including unrecognized/empty) defaults to WARNING.
fn diag_v5_severity(severity: &str) -> DiagnosticSeverity {
    match severity {
        "error" => DiagnosticSeverity::ERROR,
        "warn" => DiagnosticSeverity::WARNING,
        "info" => DiagnosticSeverity::INFORMATION,
        "hint" => DiagnosticSeverity::HINT,
        _ => DiagnosticSeverity::WARNING,
    }
}

/// One `diag_v5` row -> an LSP `Diagnostic`. Positions are already 0-based
/// (the view's contract), so unlike `to_diag` (the engine `diag` relation's
/// converter, which is 1-based) this does no line/col adjustment.
fn diag_v5_to_diagnostic(row: DiagV5Row) -> Diagnostic {
    let range = Range::new(
        Position::new(row.line.max(0) as u32, row.col.max(0) as u32),
        Position::new(row.end_line.max(0) as u32, row.end_col.max(0) as u32),
    );
    let message = match &row.hint {
        Some(hint) if !hint.is_empty() => format!("{}\nhint: {hint}", row.msg),
        _ => row.msg,
    };
    let code = (!row.code.is_empty()).then(|| lsp_types::NumberOrString::String(row.code));
    Diagnostic {
        range,
        severity: Some(diag_v5_severity(&row.severity)),
        code,
        source: Some("dl".into()),
        message,
        ..Default::default()
    }
}

/// Publish one file's diagnostics (possibly empty, for retraction) by
/// resolving `path` against `cwd` when relative (absolute paths pass
/// through unchanged) and sending `textDocument/publishDiagnostics` through
/// `sender`. Returns `Err` only when the channel itself is closed (the
/// caller treats that as "the session ended, stop polling").
fn publish_diag_v5_path(
    sender: &crossbeam_channel::Sender<Message>,
    cwd: &Path,
    path: &str,
    diagnostics: Vec<Diagnostic>,
) -> std::result::Result<(), crossbeam_channel::SendError<Message>> {
    let file_path = Path::new(path);
    let abs = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        cwd.join(file_path)
    };
    let Some(uri) = path_to_uri(&abs) else {
        return Ok(());
    };
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let notification = Notification {
        method: "textDocument/publishDiagnostics".into(),
        params: serde_json::to_value(params).unwrap_or_default(),
    };
    sender.send(Message::Notification(notification))
}

/// Query `diag` (optionally for one file), group by path, and send one
/// publishDiagnostics per file. The ticked file is always published even with
/// zero rows, so a fixed lint clears its old squiggles.
fn publish(
    connection: &Connection,
    eng: &Engine,
    root: &Path,
    only_abs: Option<&Path>,
) -> Result<()> {
    let only_rel: Option<String> = only_abs.and_then(|a| rel_of(root, a));
    let rows = eng.diags(only_rel.as_deref())?;
    // Session mute filter: drop any diag whose code the editor silenced via
    // `dl.toggleDiagCode`. Applied HERE, at the publish seam, so `--check` (which
    // reads `eng.diags` directly) is unaffected — see the module doc.
    let muted = eng.muted_codes()?;
    let mut by: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    if let Some(r) = &only_rel {
        by.entry(r.clone()).or_default();
    }
    for d in rows {
        if muted.contains(&d.code) {
            continue;
        }
        by.entry(d.path.clone()).or_default().push(to_diag(d));
    }
    // Extraction type-drops: a file whose rows the file/dir/path checks dropped
    // gets a file-level squiggle. Their `path` is repo-relative like `diag.path`,
    // so they route through the same per-file grouping. Filter to the ticked file
    // when this is a single-file republish so we never resurrect another file's
    // stale drop.
    for d in eng.extraction_drops() {
        if muted.contains(&d.code) {
            continue;
        }
        if only_rel.as_deref().map_or(true, |r| r == d.path) {
            by.entry(d.path.clone())
                .or_default()
                .push(to_diag(d.clone()));
        }
    }
    for (path, diagnostics) in by {
        let Some(uri) = path_to_uri(&root.join(&path)) else {
            continue;
        };
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        };
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
    let Some(uri) = path_to_uri(prog_abs) else {
        return Ok(());
    };
    let diagnostics: Vec<Diagnostic> = diags.iter().cloned().map(to_diag).collect();
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
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
        Some((path, _id, text, _lo, _hi)) => eng
            .definition_targets(&path, &text)
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
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(locations).unwrap_or_default(),
    )
}

/// textDocument/hover over the ref spine: cursor -> innermost located span ->
/// its string text -> engine `hover` (auto-synthesizes markdown from
/// type_entity + call_def), MERGED with any `hover_note` sink rows covering
/// the cursor position (a program-authored markdown note, see
/// `Engine::hover_notes_at`). Entity markdown and note markdown are joined by
/// a `---` rule, in that order; notes alone (no entity match) still produce a
/// hover; neither present is null, exactly as before this rel existed.
fn handle_hover(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: TextDocumentPositionParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let hit = resolve_span(eng, root, &params);
    let entity = hit.as_ref().and_then(|(path, _id, text, lo, hi)| {
        eng.hover(path, text)
            .unwrap_or(None)
            .map(|md| (path.clone(), *lo, *hi, md))
    });
    // hover_note lookup needs the rel path even when the entity resolver found
    // nothing (the cursor may sit on unlocated text a note still covers) — the
    // entity hit's path when present, else resolved independently from the URI.
    let note_path = entity
        .as_ref()
        .map(|(path, ..)| path.clone())
        .or_else(|| resolve_rel_path(root, &params.text_document.uri));
    let notes: Vec<String> = match &note_path {
        Some(path) => eng
            .hover_notes_at(path, params.position.line, params.position.character)
            .unwrap_or_default(),
        None => Vec::new(),
    };

    let md = match (
        entity.as_ref().map(|(_, _, _, md)| md.clone()),
        notes.is_empty(),
    ) {
        (Some(entity_md), true) => entity_md,
        (Some(entity_md), false) => format!("{entity_md}\n\n---\n\n{}", notes.join("\n\n---\n\n")),
        (None, false) => notes.join("\n\n---\n\n"),
        (None, true) => return Response::new_ok(req.id.clone(), serde_json::Value::Null),
    };
    // Range is the located entity span when one resolved (highlights what the
    // entity hover names); a notes-only hover carries no range.
    let range = entity.as_ref().map(|(path, lo, hi, _)| {
        let content = std::fs::read_to_string(root.join(path)).unwrap_or_default();
        span_to_range(&content, *lo, *hi)
    });
    let hover = lsp_types::Hover {
        range,
        contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
            kind: lsp_types::MarkupKind::Markdown,
            value: md,
        }),
    };
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(hover).unwrap_or_default(),
    )
}

/// `Uri` -> repo-relative path, WITHOUT resolving a located span (unlike
/// `resolve_span`, which fails when the cursor sits on unlocated text). Used
/// by the `hover_note` merge so a note can still be found at a position the
/// entity resolver has nothing to say about. Canonicalizes before stripping
/// root, matching `resolve_span`/`resolve_abs_byte`.
fn resolve_rel_path(root: &Path, uri: &Uri) -> Option<String> {
    let abs = uri_to_path(uri)?;
    let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
    rel_of(root, &abs)
}

/// Wrap a user SQL as a paginated subquery. The `LIMIT ?`/`OFFSET ?` placeholders
/// bind AFTER the caller's own positional params (the user SQL is embedded
/// verbatim, never parsed or rewritten). Malformed user SQL surfaces as the same
/// prepare error it would on a bare query.
fn paged_sql(sql: &str) -> String {
    format!("SELECT * FROM ({sql}) LIMIT ? OFFSET ?")
}

/// Wrap a user SQL as a COUNT over the same subquery — the unpaged total.
fn count_sql(sql: &str) -> String {
    format!("SELECT COUNT(*) FROM ({sql})")
}

/// Run the count subquery and read the single scalar back.
fn run_count(eng: &Engine, sql: &str, params: &[serde_json::Value]) -> Result<i64> {
    let rows = eng.query_sql(&count_sql(sql), params)?;
    Ok(rows
        .first()
        .and_then(|r| r.first())
        .and_then(|v| v.as_i64())
        .unwrap_or(0))
}

/// Custom request `dl/query`: SQL against the engine's SQLite, the same surface
/// as the daemon's `query_sql` RPC but reachable through the editor's existing
/// LSP channel (the flow-panel webview is the caller). Params:
/// `{"sql": string, "params": [scalar, ...], "limit"?: int, "offset"?: int,
/// "count"?: bool}`.
///
/// Paging is backward compatible:
///   * `count == true` -> `{"total": int}` (COUNT over the user query only).
///   * `limit` present -> `{"rows": [[col, ...]], "total": int}` — one page plus
///     the unpaged total; the page runs `SELECT * FROM (<sql>) LIMIT ? OFFSET ?`
///     with `limit`/`offset` bound after the user params (`offset` defaults 0).
///   * neither -> `{"rows": [[col, ...]]}`, exactly the pre-paging behavior (the
///     browser bridge and older clients keep working).
///
/// This path does NOT log to `_query_log`: panel auto-refresh polls it and every
/// read taking two writes on the single engine lock was hot-path waste. The
/// daemon's `query`/`query_sql` RPCs still log (`src/daemon.rs`).
fn handle_query(eng: &Engine, req: &Request) -> Response {
    let Some(sql) = req.params.get("sql").and_then(|v| v.as_str()) else {
        return Response::new_err(req.id.clone(), -32602, "missing sql".into());
    };
    let params: Vec<serde_json::Value> = req
        .params
        .get("params")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let count = req
        .params
        .get("count")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let limit = req.params.get("limit").and_then(|v| v.as_i64());
    let offset = req
        .params
        .get("offset")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    // count mode: just the total, no rows.
    if count {
        return match run_count(eng, sql, &params) {
            Ok(total) => Response::new_ok(req.id.clone(), serde_json::json!({ "total": total })),
            Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
        };
    }
    // paged mode: a window of rows plus the unpaged total.
    if let Some(limit) = limit {
        let mut page_params = params.clone();
        page_params.push(serde_json::json!(limit));
        page_params.push(serde_json::json!(offset));
        let rows = match eng.query_sql(&paged_sql(sql), &page_params) {
            Ok(rows) => rows,
            Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
        };
        return match run_count(eng, sql, &params) {
            Ok(total) => Response::new_ok(
                req.id.clone(),
                serde_json::json!({ "rows": rows, "total": total }),
            ),
            Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
        };
    }
    // legacy path: exactly today's behavior.
    match eng.query_sql(sql, &params) {
        Ok(rows) => Response::new_ok(req.id.clone(), serde_json::json!({ "rows": rows })),
        Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
    }
}

/// Custom request `dl/hookEvent`: the extension's transport for landing one
/// harness/editor event (the goto-flow recorder, chat marks, ...) in the
/// engine's `hook_event` builtin rel. Params: `{"kind": string, "session":
/// string, "seq": i64, "json": string}` (all four required) -> `{"ok": true}`.
/// Mirrors the daemon's `hook_event` RPC (`src/daemon.rs`) so both entry
/// points behave identically; under the future `LspPump::Daemon` arm this arm
/// forwards to that RPC verbatim instead of ticking the in-process engine.
fn handle_hook_event(eng: &mut Engine, prog: &ast::Program, req: &Request) -> Response {
    let p = &req.params;
    let (Some(kind), Some(session), Some(seq), Some(json)) = (
        p.get("kind").and_then(|v| v.as_str()),
        p.get("session").and_then(|v| v.as_str()),
        p.get("seq").and_then(|v| v.as_i64()),
        p.get("json").and_then(|v| v.as_str()),
    ) else {
        return Response::new_err(
            req.id.clone(),
            -32602,
            "dl/hookEvent needs kind, session, seq, json".into(),
        );
    };
    let run = (|| -> anyhow::Result<()> {
        eng.insert_hook_event(kind, session, seq, json)?;
        // The same quiet tick didSave runs, so a rule over hook_event
        // derives before the response returns.
        eng.tick(prog, true)
    })();
    match run {
        Ok(()) => Response::new_ok(req.id.clone(), serde_json::json!({ "ok": true })),
        Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
    }
}

/// Does this `workspace/executeCommand` request name the mute toggle? Used by
/// the main loop to decide whether to republish (list is read-only).
fn is_toggle_command(req: &Request) -> bool {
    req.params.get("command").and_then(|c| c.as_str()) == Some("dl.toggleDiagCode")
}

/// `workspace/executeCommand`: the diagnostic-mute affordance.
///   * `dl.toggleDiagCode` with `arguments: [code]` — flip the code in the
///     engine's `diag_mute` set; returns the new muted state (bool). The main
///     loop republishes afterward so the editor's squiggles update at once.
///   * `dl.listDiagCodes` — returns `[{ "code": string, "muted": bool }, ...]`
///     for every distinct code in `diag` (plus any muted-with-no-finding code),
///     powering the quick-pick.
/// Muting is session/db-scoped and never affects `--check` (see the module doc).
fn handle_execute_command(eng: &mut Engine, req: &Request) -> Response {
    let command = req
        .params
        .get("command")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let args = req.params.get("arguments").and_then(|a| a.as_array());
    match command {
        "dl.toggleDiagCode" => {
            let Some(code) = args.and_then(|a| a.first()).and_then(|v| v.as_str()) else {
                return Response::new_err(
                    req.id.clone(),
                    -32602,
                    "dl.toggleDiagCode needs a code string argument".into(),
                );
            };
            match eng.toggle_diag_mute(code) {
                Ok(muted) => Response::new_ok(
                    req.id.clone(),
                    serde_json::json!({ "code": code, "muted": muted }),
                ),
                Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
            }
        }
        "dl.listDiagCodes" => match eng.diag_code_states() {
            Ok(states) => {
                let list: Vec<serde_json::Value> = states
                    .into_iter()
                    .map(|(code, muted)| serde_json::json!({ "code": code, "muted": muted }))
                    .collect();
                Response::new_ok(req.id.clone(), serde_json::json!(list))
            }
            Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
        },
        other => Response::new_err(req.id.clone(), -32601, format!("unknown command: {other}")),
    }
}

/// textDocument/references over the refs lens (Track B): cursor -> `refs_lens`,
/// then flatten declarations + uses to `Location[]`. Each hit carries its OWN
/// repo, so a multi-repo hit maps to that repo's on-disk root (the `root.join`
/// multi-repo fix) instead of being mislocated under the primary root. Null when
/// the cursor is not on a located string.
fn handle_references(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: ReferenceParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some((rel, byte)) = resolve_path_byte(root, &params.text_document_position) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let lens = match eng.refs_lens(&rel, byte as usize) {
        Ok(Some(l)) => l,
        Ok(None) => return Response::new_ok(req.id.clone(), serde_json::Value::Null),
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    let mut locations = Vec::new();
    for hit in lens.declarations.iter().chain(lens.uses.iter()) {
        if let Some(loc) = refhit_location(&roots, root, hit) {
            locations.push(loc);
        }
    }
    if locations.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(locations).unwrap_or_default(),
    )
}

/// Custom request `dl/refs`: the grouped references lens (declarations, uses,
/// containing types, callers, callees) for one cursor, feeding the "dl
/// References" TreeView and the flow panel. Params:
/// `{"uri": string} | {"path": string}` (path is root-relative) plus
/// `"line"` and `"character"` (0-based, LSP convention). Returns the serialized
/// `RefLens`, or null when the cursor is not on a located identifier.
fn handle_refs(eng: &Engine, root: &Path, req: &Request) -> Response {
    let line = req.params.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = req
        .params
        .get("character")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let abs: Option<PathBuf> = if let Some(uri) = req.params.get("uri").and_then(|v| v.as_str()) {
        Uri::from_str(uri).ok().and_then(|u| uri_to_path(&u))
    } else {
        req.params
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| root.join(p))
    };
    let Some(abs) = abs else {
        return Response::new_err(req.id.clone(), -32602, "dl/refs needs a uri or path".into());
    };
    let Some((rel, byte)) = resolve_abs_byte(root, &abs, Position::new(line, character)) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    match eng.refs_lens(&rel, byte as usize) {
        Ok(Some(lens)) => Response::new_ok(
            req.id.clone(),
            serde_json::to_value(lens).unwrap_or(serde_json::Value::Null),
        ),
        Ok(None) => Response::new_ok(req.id.clone(), serde_json::Value::Null),
        Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
    }
}

/// Custom request `dl/locate`: the cheap "follow the user" point lookup (Track
/// B B4) for one cursor — cursor -> symbol -> declaration site, no
/// uses/callers/callees. Same param shape as `dl/refs`:
/// `{"uri": string} | {"path": string}` (path is root-relative) plus `"line"`
/// and `"character"` (0-based). Returns the serialized `LocateHit`, or null
/// when the cursor is not on a located identifier or the identifier resolves
/// to no symbol in either tier. Meant to be called at cursor-move cadence (the
/// extension debounces), so it does exactly one `Engine::locate` call and
/// nothing else — no fact write, no tick.
fn handle_locate(eng: &Engine, root: &Path, req: &Request) -> Response {
    let line = req.params.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let character = req
        .params
        .get("character")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let abs: Option<PathBuf> = if let Some(uri) = req.params.get("uri").and_then(|v| v.as_str()) {
        Uri::from_str(uri).ok().and_then(|u| uri_to_path(&u))
    } else {
        req.params
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| root.join(p))
    };
    let Some(abs) = abs else {
        return Response::new_err(
            req.id.clone(),
            -32602,
            "dl/locate needs a uri or path".into(),
        );
    };
    let Some((rel, byte)) = resolve_abs_byte(root, &abs, Position::new(line, character)) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    match eng.locate(&rel, byte as usize) {
        Ok(Some(hit)) => Response::new_ok(
            req.id.clone(),
            serde_json::to_value(hit).unwrap_or(serde_json::Value::Null),
        ),
        Ok(None) => Response::new_ok(req.id.clone(), serde_json::Value::Null),
        Err(e) => Response::new_err(req.id.clone(), -32603, e.to_string()),
    }
}

/// A `RefHit` -> LSP `Location`, mapping the hit's repo slug to its on-disk root
/// (`repo_roots`). An unknown slug falls back to the primary root join, matching
/// the pre-fix single-repo behavior rather than dropping the hit.
fn refhit_location(
    roots: &HashMap<String, PathBuf>,
    primary: &Path,
    hit: &crate::engine::RefHit,
) -> Option<Location> {
    let base = roots.get(&hit.repo).map(|p| p.as_path()).unwrap_or(primary);
    let uri = path_to_uri(&base.join(&hit.path))?;
    let range = Range::new(
        Position::new(hit.line, hit.col),
        Position::new(hit.end_line, hit.end_col),
    );
    Some(Location { uri, range })
}

/// textDocument/documentHighlight (B2a): cursor -> same-string spans WITHIN the
/// current file only, over the ref spine. Returns `DocumentHighlight[]` (kind
/// Text) with 0-based ranges. Null when the cursor is not on a located string.
fn handle_document_highlight(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: DocumentHighlightParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some((rel, byte)) = resolve_path_byte(root, &params.text_document_position_params) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let spans = eng.document_highlights(&rel, byte).unwrap_or_default();
    if spans.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    // One file read to turn byte spans into line/col ranges (the spans are all in
    // this file by construction).
    let content = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
    let highlights: Vec<DocumentHighlight> = spans
        .into_iter()
        .map(|(lo, hi)| DocumentHighlight {
            range: span_to_range(&content, lo, hi),
            kind: Some(DocumentHighlightKind::TEXT),
        })
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(highlights).unwrap_or_default(),
    )
}

/// workspace/symbol (B2b): query -> `type_entity` and callable names matching the
/// substring, mapped to `SymbolInformation[]`. Each hit's location is built from
/// its OWN repo root (`repo_roots` with primary-root fallback, the multi-repo
/// idiom `refhit_location` uses). Capped at 200, prefix matches first.
fn handle_workspace_symbol(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: WorkspaceSymbolParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let rows = eng
        .workspace_symbols(&params.query, 200)
        .unwrap_or_default();
    let roots = eng.repo_roots();
    #[allow(deprecated)] // SymbolInformation::deprecated is a required legacy field
    let symbols: Vec<SymbolInformation> = rows
        .iter()
        .filter_map(|row| {
            let base = roots.get(&row.repo).map(|p| p.as_path()).unwrap_or(root);
            let uri = path_to_uri(&base.join(&row.file))?;
            let line0 = (row.line - 1).max(0) as u32;
            let range = Range::new(Position::new(line0, 0), Position::new(line0, 0));
            Some(SymbolInformation {
                name: row.name.clone(),
                kind: symbol_kind(&row.kind),
                tags: None,
                deprecated: None,
                location: Location { uri, range },
                container_name: (!row.container.is_empty()).then(|| row.container.clone()),
            })
        })
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(symbols).unwrap_or_default(),
    )
}

/// textDocument/documentSymbol (B2c): `type_entity` rows for the file, nested via
/// the parent sym into a hierarchical `DocumentSymbol[]` (a method under its owner
/// type). Ranges are line-anchored zero-width at line start, matching the
/// resolved-tier RefHits. Null when the file has no located type entities.
fn handle_document_symbol(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: DocumentSymbolParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some(abs) = uri_to_path(&params.text_document.uri) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
    let Some(rel) = rel_of(root, &abs) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let rows = eng.document_symbols(&rel).unwrap_or_default();
    if rows.is_empty() {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    }
    let tree = build_symbol_tree(&rows);
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(tree).unwrap_or_default(),
    )
}

/// Nest the flat `type_entity` rows into a `DocumentSymbol` tree by the `parent`
/// sym. A row whose parent sym is present in the same file becomes that owner's
/// child; every other row is a top-level symbol. Children sort by line.
fn build_symbol_tree(rows: &[crate::engine::SymbolRow]) -> Vec<DocumentSymbol> {
    use std::collections::HashSet;
    let in_file: HashSet<&str> = rows.iter().map(|row| row.sym.as_str()).collect();
    let mut children_of: HashMap<String, Vec<usize>> = HashMap::new();
    let mut roots: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if !row.parent.is_empty() && in_file.contains(row.parent.as_str()) {
            children_of
                .entry(row.parent.clone())
                .or_default()
                .push(index);
        } else {
            roots.push(index);
        }
    }
    roots.sort_by_key(|&index| rows[index].line);
    roots
        .into_iter()
        .map(|index| symbol_node(index, rows, &children_of))
        .collect()
}

/// Build one `DocumentSymbol` node (with its children) for `rows[index]`.
fn symbol_node(
    index: usize,
    rows: &[crate::engine::SymbolRow],
    children_of: &HashMap<String, Vec<usize>>,
) -> DocumentSymbol {
    let row = &rows[index];
    let children: Vec<DocumentSymbol> = match children_of.get(&row.sym) {
        Some(child_indices) => {
            let mut ordered = child_indices.clone();
            ordered.sort_by_key(|&child| rows[child].line);
            ordered
                .into_iter()
                .map(|child| symbol_node(child, rows, children_of))
                .collect()
        }
        None => Vec::new(),
    };
    let line0 = (row.line - 1).max(0) as u32;
    let range = Range::new(Position::new(line0, 0), Position::new(line0, 0));
    #[allow(deprecated)] // DocumentSymbol::deprecated is a required legacy field
    DocumentSymbol {
        name: row.name.clone(),
        detail: None,
        kind: symbol_kind(&row.kind),
        tags: None,
        deprecated: None,
        range,
        selection_range: range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

/// Map a `type_entity`/`call_def` kind string to an LSP `SymbolKind`. The kind
/// vocabulary is struct|enum|trait|class|interface|alias|function|method|const
/// (typegraph.rs `EntityKind`); an unknown kind renders as a plain variable.
fn symbol_kind(kind: &str) -> SymbolKind {
    match kind {
        "struct" => SymbolKind::STRUCT,
        "enum" => SymbolKind::ENUM,
        "trait" | "interface" => SymbolKind::INTERFACE,
        "class" => SymbolKind::CLASS,
        "alias" => SymbolKind::TYPE_PARAMETER,
        "function" => SymbolKind::FUNCTION,
        "method" => SymbolKind::METHOD,
        "const" => SymbolKind::CONSTANT,
        _ => SymbolKind::VARIABLE,
    }
}

// ---------- call hierarchy + type hierarchy (Track B B5) ----------

/// A `HierarchyItem`'s URI (mapped through its OWN repo root, the multi-repo
/// idiom `refhit_location`/`handle_workspace_symbol` use) plus its `range`
/// (the whole declaration span when `end_line` differs from `line`, else one
/// line — the `to_diag` "no column info" convention) and zero-width
/// `selection_range` at the declaration line (no column info in either tier).
fn hierarchy_uri_range(
    roots: &HashMap<String, PathBuf>,
    primary: &Path,
    item: &HierarchyItem,
) -> Option<(Uri, Range, Range)> {
    let base = roots
        .get(&item.repo)
        .map(|p| p.as_path())
        .unwrap_or(primary);
    let uri = path_to_uri(&base.join(&item.file))?;
    let selection_range = Range::new(Position::new(item.line, 0), Position::new(item.line, 0));
    let end_line = item.end_line.max(item.line) + 1;
    let range = Range::new(Position::new(item.line, 0), Position::new(end_line, 0));
    Some((uri, range, selection_range))
}

/// `HierarchyItem` -> `CallHierarchyItem`, with the whole item round-tripped
/// into `data` so `incomingCalls`/`outgoingCalls` need no re-resolution by
/// position (the prepare/incoming/outgoing split is otherwise stateless here).
fn call_hierarchy_item(
    roots: &HashMap<String, PathBuf>,
    primary: &Path,
    item: &HierarchyItem,
) -> Option<CallHierarchyItem> {
    let (uri, range, selection_range) = hierarchy_uri_range(roots, primary, item)?;
    Some(CallHierarchyItem {
        name: item.name.clone(),
        kind: symbol_kind(&item.kind),
        tags: None,
        detail: None,
        uri,
        range,
        selection_range,
        data: serde_json::to_value(item).ok(),
    })
}

/// `HierarchyItem` -> `TypeHierarchyItem`, same `data` round-trip as
/// `call_hierarchy_item`.
fn type_hierarchy_item(
    roots: &HashMap<String, PathBuf>,
    primary: &Path,
    item: &HierarchyItem,
) -> Option<TypeHierarchyItem> {
    let (uri, range, selection_range) = hierarchy_uri_range(roots, primary, item)?;
    Some(TypeHierarchyItem {
        name: item.name.clone(),
        kind: symbol_kind(&item.kind),
        tags: None,
        detail: None,
        uri,
        range,
        selection_range,
        data: serde_json::to_value(item).ok(),
    })
}

/// A `HierarchyItem`'s `data` field (round-tripped from a prior prepare/
/// incoming/outgoing/supertypes/subtypes response) back to the typed struct.
/// None when the client sent no `data` or a malformed one (a stale item from
/// a since-restarted server, or a non-dl call/type hierarchy provider mixed
/// in — the request is dropped rather than mis-resolved).
fn hierarchy_item_from_data(data: &Option<serde_json::Value>) -> Option<HierarchyItem> {
    data.clone().and_then(|d| serde_json::from_value(d).ok())
}

/// Ranges within `edge`'s neighbor's file: `from_lines` when the tier found
/// call-site lines, else the neighbor's own declaration line as a fallback (an
/// empty `from_ranges` array is legal per spec but several clients render no
/// highlight at all without one).
fn hierarchy_from_ranges(edge: &crate::engine::HierarchyCallEdge, item_range: Range) -> Vec<Range> {
    if edge.from_lines.is_empty() {
        return vec![item_range];
    }
    edge.from_lines
        .iter()
        .map(|&line| Range::new(Position::new(line, 0), Position::new(line, 0)))
        .collect()
}

/// `textDocument/prepareCallHierarchy` (B5): cursor -> a callable
/// `HierarchyItem`, wrapped as the single-element array the LSP result shape
/// wants. Null when the cursor is not on a located identifier or the
/// identifier is not a callable in either tier (see
/// `Engine::call_hierarchy_prepare`).
fn handle_call_hierarchy_prepare(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: CallHierarchyPrepareParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some((rel, byte)) = resolve_path_byte(root, &params.text_document_position_params) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let item = match eng.call_hierarchy_prepare(&rel, byte as usize) {
        Ok(Some(item)) => item,
        Ok(None) => return Response::new_ok(req.id.clone(), serde_json::Value::Null),
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    match call_hierarchy_item(&roots, root, &item) {
        Some(chi) => Response::new_ok(req.id.clone(), serde_json::json!([chi])),
        None => Response::new_ok(req.id.clone(), serde_json::Value::Null),
    }
}

/// `callHierarchy/incomingCalls` (B5): the request item's `data` round-trips to
/// a `HierarchyItem`, then one 1-hop caller lookup (`Engine::
/// call_hierarchy_incoming` — no closure, tier-matched to how the item
/// resolved).
fn handle_call_hierarchy_incoming(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: CallHierarchyIncomingCallsParams = match serde_json::from_value(req.params.clone())
    {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some(item) = hierarchy_item_from_data(&params.item.data) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let edges = match eng.call_hierarchy_incoming(&item) {
        Ok(edges) => edges,
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    let calls: Vec<CallHierarchyIncomingCall> = edges
        .iter()
        .filter_map(|edge| {
            let from = call_hierarchy_item(&roots, root, &edge.item)?;
            let from_ranges = hierarchy_from_ranges(edge, from.range);
            Some(CallHierarchyIncomingCall { from, from_ranges })
        })
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(calls).unwrap_or_default(),
    )
}

/// `callHierarchy/outgoingCalls` (B5): same shape as
/// `handle_call_hierarchy_incoming`, the callee direction.
fn handle_call_hierarchy_outgoing(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: CallHierarchyOutgoingCallsParams = match serde_json::from_value(req.params.clone())
    {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some(item) = hierarchy_item_from_data(&params.item.data) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let edges = match eng.call_hierarchy_outgoing(&item) {
        Ok(edges) => edges,
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    let calls: Vec<CallHierarchyOutgoingCall> = edges
        .iter()
        .filter_map(|edge| {
            let to = call_hierarchy_item(&roots, root, &edge.item)?;
            let from_ranges = hierarchy_from_ranges(edge, to.range);
            Some(CallHierarchyOutgoingCall { to, from_ranges })
        })
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(calls).unwrap_or_default(),
    )
}

/// `textDocument/prepareTypeHierarchy` (B5): cursor -> a type-shaped
/// `HierarchyItem` (struct/enum/trait/class/interface only — see
/// `Engine::type_hierarchy_prepare`), wrapped as a single-element array.
fn handle_type_hierarchy_prepare(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: TypeHierarchyPrepareParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some((rel, byte)) = resolve_path_byte(root, &params.text_document_position_params) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let item = match eng.type_hierarchy_prepare(&rel, byte as usize) {
        Ok(Some(item)) => item,
        Ok(None) => return Response::new_ok(req.id.clone(), serde_json::Value::Null),
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    match type_hierarchy_item(&roots, root, &item) {
        Some(thi) => Response::new_ok(req.id.clone(), serde_json::json!([thi])),
        None => Response::new_ok(req.id.clone(), serde_json::Value::Null),
    }
}

/// `typeHierarchy/supertypes` (B5): the request item's `data` round-trips to a
/// `HierarchyItem`, then one 1-hop supertype lookup (`type_link` kind `impl`
/// plus interface-owned `generic`, or `scip_impl` for a compiler-tier item —
/// see `Engine::type_hierarchy_supertypes`).
fn handle_type_hierarchy_supertypes(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: TypeHierarchySupertypesParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some(item) = hierarchy_item_from_data(&params.item.data) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let supers = match eng.type_hierarchy_supertypes(&item) {
        Ok(supers) => supers,
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    let items: Vec<TypeHierarchyItem> = supers
        .iter()
        .filter_map(|s| type_hierarchy_item(&roots, root, s))
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(items).unwrap_or_default(),
    )
}

/// `typeHierarchy/subtypes` (B5): same shape as
/// `handle_type_hierarchy_supertypes`, the implementer direction.
fn handle_type_hierarchy_subtypes(eng: &Engine, root: &Path, req: &Request) -> Response {
    let params: TypeHierarchySubtypesParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => return Response::new_err(req.id.clone(), -32602, e.to_string()),
    };
    let Some(item) = hierarchy_item_from_data(&params.item.data) else {
        return Response::new_ok(req.id.clone(), serde_json::Value::Null);
    };
    let subs = match eng.type_hierarchy_subtypes(&item) {
        Ok(subs) => subs,
        Err(e) => return Response::new_err(req.id.clone(), -32603, e.to_string()),
    };
    let roots = eng.repo_roots();
    let items: Vec<TypeHierarchyItem> = subs
        .iter()
        .filter_map(|s| type_hierarchy_item(&roots, root, s))
        .collect();
    Response::new_ok(
        req.id.clone(),
        serde_json::to_value(items).unwrap_or_default(),
    )
}

/// Send the outbound `dl/graphChanged` pulse (A5 push-refresh v1). Params is a
/// JSON object (not null) so a later version can add tick/moved-rel fields
/// without breaking the wire contract.
fn send_graph_changed(connection: &Connection) -> Result<()> {
    connection.sender.send(Message::Notification(Notification {
        method: "dl/graphChanged".into(),
        params: serde_json::json!({}),
    }))?;
    Ok(())
}

/// (uri, position) -> the innermost located span under the cursor, as
/// (repo-relative path, string_id, text, lo, hi). Reads the file from disk
/// (save-driven: disk is truth, same as extraction) to convert the position to
/// a byte offset.
fn resolve_span(
    eng: &Engine,
    root: &Path,
    pos: &TextDocumentPositionParams,
) -> Option<(String, String, String, u32, u32)> {
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

/// (uri, position) -> (repo-relative path, byte offset), reading the file from
/// disk to convert the position. The refs lens does its own span resolution
/// (it needs the path + byte, not the located span), so this is `resolve_span`
/// without the `span_at` lookup.
fn resolve_path_byte(root: &Path, pos: &TextDocumentPositionParams) -> Option<(String, u32)> {
    let abs = uri_to_path(&pos.text_document.uri)?;
    resolve_abs_byte(root, &abs, pos.position)
}

/// (abs path, position) -> (repo-relative path, byte offset). Canonicalizes the
/// path (macOS `/var` -> `/private/var`) before stripping the root, the same as
/// `resolve_span`. None when the file is outside the root or unreadable.
fn resolve_abs_byte(root: &Path, abs: &Path, position: Position) -> Option<(String, u32)> {
    let abs = std::fs::canonicalize(abs).unwrap_or_else(|_| abs.to_path_buf());
    let rel = rel_of(root, &abs)?;
    let content = std::fs::read_to_string(&abs).ok()?;
    let byte = position_to_byte(&content, position)?;
    Some((rel, byte))
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
    let col_bytes: usize = rest[..line_end]
        .chars()
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
    diags
        .iter()
        .map(|d| {
            let (line, col) = if d.span == (0, 0) {
                (1, 0)
            } else {
                byte_to_line_col(src, d.span.0)
            };
            let (end_line, end_col) = if d.span == (0, 0) {
                (1, 0)
            } else {
                byte_to_line_col(src, d.span.1)
            };
            DiagRow {
                path: d.path.clone(),
                line: line as i64,
                col: col as i64,
                end_line: end_line as i64,
                end_col: end_col as i64,
                severity: d.severity.as_str().to_string(),
                code: d.code.clone(),
                msg: d.msg.clone(),
                hint: None,
            }
        })
        .collect()
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
        if i >= byte {
            break;
        }
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
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
        Range::new(
            Position::new(line0, d.col.max(0) as u32),
            Position::new(end0, d.end_col.max(0) as u32),
        )
    };
    let message = match &d.hint {
        Some(h) => format!("{}\nhint: {h}", d.msg),
        None => d.msg,
    };
    let code = (!d.code.is_empty()).then(|| lsp_types::NumberOrString::String(d.code));
    Diagnostic {
        range,
        severity,
        code,
        source: Some("dl".into()),
        message,
        ..Default::default()
    }
}

/// abs path -> path relative to root, forward-slashed (matches stored diag.path).
fn rel_of(root: &Path, abs: &Path) -> Option<String> {
    abs.strip_prefix(root)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
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
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'/' | b'-' | b'.' | b'_' | b'~' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => {
                out.push(b as char)
            }
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
                    start: Position {
                        line,
                        character: col,
                    },
                    end: Position {
                        line: end_line,
                        character: end_col,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("dl".to_string()),
                message: msg.clone(),
                ..Default::default()
            }
        })
        .collect();
    let file_uri = Uri::from_str(&format!("file://{}", abs.display()))
        .unwrap_or(Uri::from_str("file:///").unwrap());
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
            let snippet = source_text.get(start_byte..end_byte).unwrap_or("").trim();
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

#[cfg(test)]
mod tests {
    use super::{count_sql, paged_sql};
    use crate::lower::txt_tbl;

    #[test]
    fn paged_sql_wraps_as_subquery_with_placeholders() {
        assert_eq!(
            paged_sql(&format!(
                "SELECT method FROM {} ORDER BY ts",
                txt_tbl("query_log")
            )),
            format!(
                "SELECT * FROM (SELECT method FROM {} ORDER BY ts) LIMIT ? OFFSET ?",
                txt_tbl("query_log")
            ),
        );
    }

    #[test]
    fn count_sql_wraps_as_count_subquery() {
        assert_eq!(
            count_sql(&format!(
                "SELECT method FROM {} WHERE method = ?",
                txt_tbl("query_log")
            )),
            format!(
                "SELECT COUNT(*) FROM (SELECT method FROM {} WHERE method = ?)",
                txt_tbl("query_log")
            ),
        );
    }
}
