//! RPC dispatch: `handle_request`/`dispatch_root` routing, eval paths, and
//! the JSON renderers (relocated from `daemon.rs`; decomposition plan
//! step 6).
use super::*;

/// One access-log line per inbound request, either transport. The apache-
/// style line this arc exists for: request id, which door it came through,
/// the RPC method, which root it addressed (empty = config view / not yet
/// resolved), how long dispatch took, and whether it succeeded. Byte counts
/// are the caller's to add when cheap (the HTTP body length; the UDS path
/// already has the framed body in hand) — optional, so `0` means "not
/// counted" rather than "empty body".
pub(crate) fn log_access(
    surface: &str,
    req_id: &str,
    method: &str,
    root: Option<&str>,
    ms: u64,
    ok: bool,
    bytes_in: usize,
    bytes_out: usize,
) {
    tracing::info!(
        req_id,
        surface,
        method,
        root = root.unwrap_or(""),
        ms,
        ok,
        bytes_in,
        bytes_out,
        "[access]"
    );
}

pub(crate) fn parse_request(v: Value) -> Option<Request> {
    let id = v.get("id")?.as_u64()?;
    let method = v.get("method")?.as_str()?.to_string();
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    Some(Request { id, method, params })
}

/// The `root` envelope key: an absolute path in the request's params. Absent =
/// the config view.
fn req_root(req: &Request) -> Option<String> {
    req.params
        .get("root")
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Dispatch one request. Transport-agnostic and synchronous — both the UDS
/// connection task and the axum `/rpc` handler call it through `spawn_blocking`,
/// since it takes `lock_eng` / the `prog` lock. `subscribe` is NOT handled here:
/// both transports intercept it (the UDS task registers the write half with the
/// subscriber pump; the HTTP path rejects it), so a `subscribe` never reaches
/// this function.
pub(crate) fn handle_request(d: &Arc<Daemon>, req: &Request, req_id: &str) -> Response {
    // Anything synchronous on THIS thread for the rest of the call — an
    // inline tick from `run_eval` (the `eval`/`load mode=once` path), a
    // `JobRow` built here — can read this id back via `crate::reqid::current`
    // and tag its own event/row with it. Dropped (restoring whatever was
    // active before, normally nothing) on every return path via RAII.
    let _reqid_scope = crate::reqid::scope(req_id);
    // ----- process-level methods (no root routing) -----
    match req.method.as_str() {
        "shutdown" => return Response::ok(req.id, json!({"ok": true})),
        "add_root" => {
            let Some(path) = req_root(req) else {
                return Response::err(req.id, INVALID_PARAMS, "add_root needs root");
            };
            return match d.add_root(Path::new(&path)) {
                Ok(sr) => Response::ok(
                    req.id,
                    json!({
                        "root": sr.root.to_string_lossy(),
                        "key": sr.key,
                        "tick_count": sr.tick_count.load(Ordering::Relaxed),
                    }),
                ),
                Err(e) => Response::err(req.id, INVALID_PARAMS, format!("{e}")),
            };
        }
        "drop_root" => {
            let Some(path) = req_root(req) else {
                return Response::err(req.id, INVALID_PARAMS, "drop_root needs root");
            };
            let purge = req
                .params
                .get("purge")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            return match d.drop_root(Path::new(&path), purge) {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            };
        }
        // `ping`/`status` WITHOUT a root return the process summary + the roots list.
        "ping" | "status" if req_root(req).is_none() => {
            return daemon_summary(d, req);
        }
        // `jobs` lists the whole (process-wide) job table, newest first. Answered
        // WITHOUT the engine lock: a read-only connection straight on the job db.
        "jobs" => {
            return match d.jobs.list() {
                Ok(rows) => Response::ok(
                    req.id,
                    json!({"jobs": rows.iter().map(job_row_json).collect::<Vec<_>>()}),
                ),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            };
        }
        _ => {}
    }

    // ----- root-scoped methods -----
    let sr = match d.resolve(req_root(req).as_deref()) {
        Ok(sr) => sr,
        Err(e) => return Response::err(req.id, INVALID_PARAMS, e),
    };
    let resp = dispatch_root(&sr, d, req);
    sr.touch();
    resp
}

/// Process-level summary for a rootless `ping`/`status`: build identity + every
/// served root with its tick count.
fn daemon_summary(d: &Arc<Daemon>, req: &Request) -> Response {
    let act = crate::activity::snapshot();
    let roots: Vec<Value> = lock(&d.roots)
        .values()
        .map(|sr| {
            let (fx_failed, fx_orphaned) = lock(&sr.eng).effect_status_counts().unwrap_or((0, 0));
            json!({
                "root": sr.root.to_string_lossy(),
                "key": sr.key,
                "tick_count": sr.tick_count.load(Ordering::Relaxed),
                "program": sr.program_display,
                "settled": sr.settled.load(Ordering::Relaxed),
                "cold_start_pending": sr.cold_pending.load(Ordering::Relaxed),
                "effects_failed": fx_failed,
                "effects_orphaned": fx_orphaned,
            })
        })
        .collect();
    Response::ok(
        req.id,
        json!({
            "ok": true,
            "build_id": &*d.build_id,
            "home": d.home.to_string_lossy(),
            "config_tick_count": d.config.tick_count.load(Ordering::Relaxed),
            "root_count": roots.len(),
            "roots": roots,
            "activity": {
                "phase": act.phase.as_str(),
                "detail": act.detail,
                "program": act.program,
                "tick": act.tick,
                "elapsed_ms": act.elapsed_ms,
            },
        }),
    )
}

/// Dispatch a root-scoped method against the resolved served root.
fn dispatch_root(sr: &Arc<ServedRoot>, _d: &Arc<Daemon>, req: &Request) -> Response {
    match req.method.as_str() {
        "ping" => {
            let act = crate::activity::snapshot();
            Response::ok(
                req.id,
                json!({
                    "ok": true,
                    "build_id": &*sr.shared.build_id,
                    "root": sr.root.to_string_lossy(),
                    "key": sr.key,
                    "tick_count": sr.tick_count.load(Ordering::Relaxed),
                    "settled": sr.settled.load(Ordering::Relaxed),
                    "cold_start_pending": sr.cold_pending.load(Ordering::Relaxed),
                    "program": sr.program_display,
                    "program_files": lock(&sr.program_files).iter()
                        .map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                    "activity": {
                        "phase": act.phase.as_str(),
                        "detail": act.detail,
                        "program": act.program,
                        "tick": act.tick,
                        "elapsed_ms": act.elapsed_ms,
                    },
                }),
            )
        }
        "await_quiescent" => {
            let timeout_ms = req
                .params
                .get("timeout_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(30_000);
            let deadline = Instant::now() + Duration::from_millis(timeout_ms);
            loop {
                if sr.settled.load(Ordering::Relaxed) {
                    return Response::ok(
                        req.id,
                        json!({
                            "settled": true,
                            "tick_count": sr.tick_count.load(Ordering::Relaxed),
                        }),
                    );
                }
                if Instant::now() >= deadline {
                    return Response::ok(
                        req.id,
                        json!({
                            "settled": false,
                            "tick_count": sr.tick_count.load(Ordering::Relaxed),
                        }),
                    );
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        "query" => {
            // Fast path: answer from committed SQLite state off a read-only
            // connection, no `lock_eng`. `None` = aggregate query / no on-disk
            // db, which needs the engine (falls through below).
            match crate::daemon_read::query(&sr.read_view()) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let prog = lock(&sr.prog);
                    let eng = lock_eng(sr, &req.method);
                    _ = eng.log_query("daemon", "query", "", "[]");
                    _ = crate::rels::refresh_query_log(&eng);
                    match eng.run_queries_capture(&prog) {
                        Ok(results) => Response::ok(
                            req.id,
                            json!({"results": results.iter().map(|r| json!({
                            "rel": r.rel, "columns": r.columns, "rows": r.rows,
                        })).collect::<Vec<_>>()}),
                        ),
                        Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
                    }
                }
            }
        }
        "diag" => {
            let only = req.params.get("path").and_then(|p| p.as_str());
            let eng = lock_eng(sr, &req.method);
            match eng.diags(only) {
                // R7: alongside the diag rows, ship the `diag_stage` routes
                // ([code, stage] pairs) and the latest-turn `agent_touch` paths
                // so the client filters by stage (and, for the hook's
                // agent-turn surface, by touched path) in one round trip.
                Ok(rows) => {
                    let stages = eng.rel_rows("diag_stage", 2);
                    let touch: Vec<String> = eng
                        .rel_rows("agent_touch", 3)
                        .into_iter()
                        .filter_map(|r| r.into_iter().nth(2))
                        .collect();
                    Response::ok(
                        req.id,
                        json!({
                            "rows": rows.iter().map(diag_to_json).collect::<Vec<_>>(),
                            "stages": stages,
                            "touch": touch,
                        }),
                    )
                }
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "definition" => {
            let file = match req.params.get("file").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing file"),
            };
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            let eng = lock_eng(sr, &req.method);
            match eng.definition_targets(file, text) {
                Ok(targets) => Response::ok(
                    req.id,
                    json!({"targets": targets.iter()
                    .map(|(f, l)| json!([f, l])).collect::<Vec<_>>()}),
                ),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "hover" => {
            let file = req
                .params
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            let eng = lock_eng(sr, &req.method);
            match eng.hover(file, text) {
                Ok(md) => Response::ok(req.id, json!({"markdown": md})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "schema" => {
            // Shapes come from the read-path snapshot (refreshed each program
            // load); no `lock_eng`.
            Response::ok(req.id, crate::daemon_read::schema(&sr.read_view()))
        }
        "query_rel" => {
            let rel = match req.params.get("rel").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing rel"),
            };
            match crate::daemon_read::query_rel(&sr.read_view(), rel) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let eng = lock_eng(sr, &req.method);
                    let Some(meta) = eng.rels.get(rel) else {
                        return Response::err(
                            req.id,
                            INVALID_PARAMS,
                            format!("unknown relation {rel:?}"),
                        );
                    };
                    let cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
                    let rows = eng.rel_rows(rel, cols.len());
                    Response::ok(req.id, json!({"columns": cols, "rows": rows}))
                }
            }
        }
        "what" => {
            let anchor = match req.params.get("anchor").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing anchor"),
            };
            let limit = req
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let offset = req
                .params
                .get("offset")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let eng = lock_eng(sr, &req.method);
            let out = crate::anchor::what(&eng, anchor, limit, offset);
            Response::ok(
                req.id,
                json!({
                    "columns": out.columns, "rows": out.rows,
                    "total": out.total, "notes": out.notes,
                }),
            )
        }
        "summary" => {
            let path = match req.params.get("path").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing path"),
            };
            let eng = lock_eng(sr, &req.method);
            let out = crate::anchor::summary(&eng, path);
            Response::ok(
                req.id,
                json!({
                    "columns": out.columns, "rows": out.rows,
                    "total": out.total, "notes": out.notes,
                }),
            )
        }
        "q" => {
            let verb = match req.params.get("verb").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing verb"),
            };
            let arg = match req.params.get("target").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing target"),
            };
            let limit = req
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let offset = req
                .params
                .get("offset")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            match run_q_eval(sr, verb, arg, limit, offset) {
                Ok(v) => Response::ok(req.id, v),
                Err((code, msg)) => Response::err(req.id, code, msg),
            }
        }
        "query_sql" => {
            let sql_raw = match req.params.get("sql").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing sql"),
            };
            let params: Vec<Value> = req
                .params
                .get("params")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            match crate::daemon_read::query_sql(&sr.read_view(), sql_raw, &params) {
                Some(Ok(v)) => Response::ok(req.id, v),
                Some(Err((code, msg))) => Response::err(req.id, code, msg),
                None => {
                    let eng = lock_eng(sr, &req.method);
                    let params_json =
                        serde_json::to_string(&params).unwrap_or_else(|_| "[]".into());
                    _ = eng.log_query("daemon", "query_sql", sql_raw, &params_json);
                    _ = crate::rels::refresh_query_log(&eng);
                    match eng.query_sql(sql_raw, &params) {
                        Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                        Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
                    }
                }
            }
        }
        "mcp_request" => {
            let p = &req.params;
            let (Some(in_rel), Some(out_rel), Some(rid), Some(method)) = (
                p.get("in_rel").and_then(|v| v.as_str()),
                p.get("out_rel").and_then(|v| v.as_str()),
                p.get("id").and_then(|v| v.as_str()),
                p.get("method").and_then(|v| v.as_str()),
            ) else {
                return Response::err(
                    req.id,
                    INVALID_PARAMS,
                    "mcp_request needs in_rel, out_rel, id, method",
                );
            };
            let args = p.get("params").and_then(|v| v.as_str()).unwrap_or("null");
            let prog = lock(&sr.prog);
            for (rel, dir) in [
                (in_rel, crate::ast::PortDir::In),
                (out_rel, crate::ast::PortDir::Out),
            ] {
                match crate::mcp::port_decl(&prog, rel) {
                    Some(port) if port.dir == dir && port.class == "rpc" => {}
                    _ => {
                        return Response::err(
                            req.id,
                            INVALID_PARAMS,
                            format!(
                                "rel {rel} is not an @{}(rpc) port in the daemon's loaded program",
                                if dir == crate::ast::PortDir::In {
                                    "in"
                                } else {
                                    "out"
                                }
                            ),
                        )
                    }
                }
            }
            let mut eng = lock_eng(sr, &req.method);
            let run = (|| -> anyhow::Result<Vec<(String, String)>> {
                eng.inject_rpc(in_rel, rid, method, args)?;
                eng.tick(&prog, true)?;
                eng.drain_rpc(out_rel, in_rel)
            })();
            sr.tick_count.fetch_add(1, Ordering::Relaxed);
            match run {
                Ok(rows) => Response::ok(req.id, json!({"rows": rows})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "mcp_retire" => {
            let Some(in_rel) = req.params.get("in_rel").and_then(|v| v.as_str()) else {
                return Response::err(req.id, INVALID_PARAMS, "mcp_retire needs in_rel");
            };
            let ids: Vec<String> = req
                .params
                .get("ids")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            {
                let prog = lock(&sr.prog);
                match crate::mcp::port_decl(&prog, in_rel) {
                    Some(port) if port.dir == crate::ast::PortDir::In && port.class == "rpc" => {}
                    _ => {
                        return Response::err(
                            req.id,
                            INVALID_PARAMS,
                            format!(
                        "rel {in_rel} is not an @in(rpc) port in the daemon's loaded program"),
                        )
                    }
                }
            }
            let mut eng = lock_eng(sr, &req.method);
            match eng.retire_rpc(in_rel, &ids) {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "hook_event" => {
            let p = &req.params;
            let (Some(kind), Some(session), Some(json)) = (
                p.get("kind").and_then(|v| v.as_str()),
                p.get("session").and_then(|v| v.as_str()),
                p.get("json").and_then(|v| v.as_str()),
            ) else {
                return Response::err(
                    req.id,
                    INVALID_PARAMS,
                    "hook_event needs kind, session, json",
                );
            };
            let seq = p.get("seq").and_then(|v| v.as_i64()).unwrap_or_else(|| {
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0)
            });
            let prog = lock(&sr.prog);
            let mut eng = lock_eng(sr, &req.method);
            let run = (|| -> anyhow::Result<()> {
                eng.insert_hook_event(kind, session, seq, json)?;
                eng.tick(&prog, true)
            })();
            sr.tick_count.fetch_add(1, Ordering::Relaxed);
            match run {
                Ok(()) => Response::ok(req.id, json!({"ok": true})),
                Err(e) => Response::err(req.id, INTERNAL_ERROR, format!("{e}")),
            }
        }
        "eval" => {
            let text = match req.params.get("text").and_then(|v| v.as_str()) {
                Some(s) => s,
                None => return Response::err(req.id, INVALID_PARAMS, "missing text"),
            };
            match run_eval(sr, text) {
                Ok(v) => Response::ok(req.id, v),
                Err((code, msg)) => Response::err(req.id, code, msg),
            }
        }
        "load" => {
            let path = match req.params.get("path").and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => return Response::err(req.id, INVALID_PARAMS, "missing path"),
            };
            let mode = req
                .params
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("watched");
            match mode {
                "once" => {
                    let text = match std::fs::read_to_string(&path) {
                        Ok(t) => t,
                        Err(e) => {
                            return Response::err(
                                req.id,
                                INVALID_PARAMS,
                                format!("read {path}: {e}"),
                            )
                        }
                    };
                    match run_eval(sr, &text) {
                        Ok(v) => Response::ok(req.id, v),
                        Err((code, msg)) => Response::err(req.id, code, msg),
                    }
                }
                "watched" => {
                    let canon = match std::fs::canonicalize(&path) {
                        Ok(c) => c,
                        Err(e) => {
                            return Response::err(
                                req.id,
                                INVALID_PARAMS,
                                format!("canonicalize {path}: {e}"),
                            )
                        }
                    };
                    let already = {
                        let mut pf = lock(&sr.program_files);
                        let dup = pf.iter().any(|f| f == &canon);
                        if !dup {
                            pf.push(canon.clone());
                        }
                        dup
                    };
                    match sr.reload_program() {
                        Ok(()) => {
                            if let Err(e) = lock_eng(sr, &req.method)
                                .save_program_meta(&lock(&sr.program_files).clone())
                            {
                                tracing::warn!("[{}] save_program_meta: {e}", sr.root_label());
                            }
                            let files: Vec<String> = lock(&sr.program_files)
                                .iter()
                                .map(|f| f.to_string_lossy().into_owned())
                                .collect();
                            Response::ok(
                                req.id,
                                json!({
                                    "loaded": canon.to_string_lossy(),
                                    "already_loaded": already,
                                    "program_files": files,
                                }),
                            )
                        }
                        Err(e) => {
                            if !already {
                                lock(&sr.program_files).retain(|f| f != &canon);
                            }
                            Response::err(req.id, INTERNAL_ERROR, format!("reload: {e}"))
                        }
                    }
                }
                other => Response::err(
                    req.id,
                    INVALID_PARAMS,
                    format!("mode must be watched|once, got {other}"),
                ),
            }
        }
        other => Response::err(req.id, METHOD_NOT_FOUND, format!("unknown method: {other}")),
    }
}

/// Evaluate a scratch `.dl` snippet without touching the live engine or db.
fn run_eval(sr: &Arc<ServedRoot>, text: &str) -> Result<Value, (i64, String)> {
    let toks = crate::lex::lex(text).map_err(|e| (INVALID_PARAMS, format!("lex: {e}")))?;
    let snippet = crate::parse::parse(toks).map_err(|e| (INVALID_PARAMS, format!("parse: {e}")))?;
    let snippet_queries: Vec<crate::ast::Item> = snippet
        .items
        .iter()
        .filter(|i| matches!(i, crate::ast::Item::Query(_)))
        .cloned()
        .collect();

    let mut merged = {
        let base = lock(&sr.prog);
        Program {
            items: base.items.iter().cloned().chain(snippet.items).collect(),
        }
    };
    let diags = crate::typecheck::check_and_normalize(&mut merged, "<scratch>");
    let diag_json = |x: &crate::ast::TypeDiag| json!({"severity": x.severity.as_str(), "code": x.code, "message": x.msg});
    let type_errs: Vec<Value> = diags
        .iter()
        .filter(|x| x.severity == crate::ast::Severity::Error)
        .map(diag_json)
        .collect();
    if !type_errs.is_empty() {
        return Ok(json!({"ok": false, "results": [], "diagnostics": type_errs}));
    }

    let conn = db::open(None).map_err(|e| (INTERNAL_ERROR, format!("db: {e}")))?;
    let mut eng = Engine::new(conn, sr.root.clone());
    eng.set_repos(served_repos(sr.key.is_none()));
    eng.tick(&merged, true)
        .map_err(|e| (INTERNAL_ERROR, format!("tick: {e}")))?;

    let qprog = Program {
        items: snippet_queries,
    };
    let results = eng
        .run_queries_capture(&qprog)
        .map_err(|e| (INTERNAL_ERROR, format!("query: {e}")))?;
    let rel_diags = eng.diags(None).unwrap_or_default();
    let all_diags: Vec<Value> = diags
        .iter()
        .map(diag_json)
        .chain(rel_diags.iter().map(diag_to_json))
        .collect();
    Ok(json!({
        "ok": true,
        "results": results.iter().map(|r| json!({
            "rel": r.rel, "columns": r.columns, "rows": r.rows,
        })).collect::<Vec<_>>(),
        "diagnostics": all_diags,
    }))
}

/// Evaluate a `dl q <verb>` against a SCRATCH engine (never the served one):
/// build the embedded verb program (with the `target` fact injected), merge it
/// onto the base program so it inherits the served scan corpus, tick a fresh
/// in-memory engine, capture the verb's `?` query, and shape it into the
/// `{columns, rows, total, notes}` envelope with the `resolve_name` note. Mirrors
/// `run_eval`; the daemon-side of the `dl q` runner.
fn run_q_eval(
    sr: &Arc<ServedRoot>,
    verb: &str,
    arg: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Value, (i64, String)> {
    let Some(spec) = crate::verbs::find(verb) else {
        return Err((
            INVALID_PARAMS,
            format!(
                "unknown verb {verb:?}; available verbs: {}",
                crate::verbs::verb_list()
            ),
        ));
    };
    let snippet = crate::verbs::verb_program(spec, arg)
        .map_err(|e| (INVALID_PARAMS, format!("verb program: {e}")))?;
    let snippet_queries: Vec<crate::ast::Item> = snippet
        .items
        .iter()
        .filter(|i| matches!(i, crate::ast::Item::Query(_)))
        .cloned()
        .collect();
    let mut merged = {
        let base = lock(&sr.prog);
        Program {
            items: base.items.iter().cloned().chain(snippet.items).collect(),
        }
    };
    let diags = crate::typecheck::check_and_normalize(&mut merged, "<verb>");
    if diags
        .iter()
        .any(|d| d.severity == crate::ast::Severity::Error)
    {
        let msgs: Vec<String> = diags
            .iter()
            .filter(|d| d.severity == crate::ast::Severity::Error)
            .map(|d| d.msg.clone())
            .collect();
        return Err((
            INTERNAL_ERROR,
            format!("verb typecheck: {}", msgs.join("; ")),
        ));
    }
    let conn = db::open(None).map_err(|e| (INTERNAL_ERROR, format!("db: {e}")))?;
    let mut eng = Engine::new(conn, sr.root.clone());
    eng.set_repos(served_repos(sr.key.is_none()));
    eng.tick(&merged, true)
        .map_err(|e| (INTERNAL_ERROR, format!("tick: {e}")))?;
    let qprog = Program {
        items: snippet_queries,
    };
    let results = eng
        .run_queries_capture(&qprog)
        .map_err(|e| (INTERNAL_ERROR, format!("query: {e}")))?;
    let (columns, rows) = crate::verbs::shape(results);
    let total = rows.len();
    let rows = crate::verbs::page(rows, limit, offset);
    let notes = vec![crate::verbs::resolve_note(&eng, arg)];
    Ok(json!({"columns": columns, "rows": rows, "total": total, "notes": notes}))
}

fn diag_to_json(d: &DiagRow) -> Value {
    json!({
        "path": d.path, "line": d.line, "col": d.col,
        "endLine": d.end_line, "endCol": d.end_col,
        "severity": d.severity, "code": d.code, "message": d.msg,
        "hint": d.hint,
    })
}

fn job_row_json(r: &crate::jobq::JobListRow) -> Value {
    json!({
        "key": r.key, "kind": r.kind, "root": r.root, "state": r.state,
        "priority": r.priority, "attempts": r.attempts, "run_at": r.run_at,
        "enqueued_at": r.enqueued_at, "started_at": r.started_at,
        "finished_at": r.finished_at, "last_error": r.last_error,
    })
}
