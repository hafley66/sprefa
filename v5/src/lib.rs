pub mod agent;
pub mod ast;
pub mod comment;
pub mod config;
pub mod corpus;
pub mod cst;
pub mod daemon;
pub mod datapath;
pub mod db;
pub mod desc;
pub mod embed;
pub mod engine;
pub mod effect;
pub mod frontend;
pub mod hook;
pub mod ingest;
pub mod ktpath;
pub mod lex;
pub mod lower;
pub mod lsp;
pub mod modgraph;
pub mod parse;
pub mod propose;
pub mod rpc;
pub mod refactor;
pub mod rels;
pub mod repo;
pub mod rspath;
pub mod scc;
pub mod scip_import;
pub mod setup;
pub mod sg;
pub mod spine;
pub mod tray;
pub mod trace;
pub mod typecheck;
pub mod typegraph;

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Resolve the program file set: an explicit path, or with no positional the
/// repo-local discovery convention `<root>/.dl/*.dl` (lexicographic file
/// order). A missing or empty directory is a loud error: a typo'd dir must
/// never let `--check` pass green by checking nothing.
pub fn resolve_programs(program: Option<&str>, root: &Path) -> Result<Vec<PathBuf>> {
    if let Some(p) = program { return Ok(vec![PathBuf::from(p)]); }
    let dir = root.join(".dl");
    let rd = std::fs::read_dir(&dir)
        .map_err(|_| anyhow::anyhow!("no program argument and no {} directory", dir.display()))?;
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "dl")).collect();
    if files.is_empty() { anyhow::bail!("no .dl files in {}", dir.display()); }
    files.sort();
    Ok(files)
}

/// Parse each file, splice the items in file order into one program, then run
/// the lower-time type passes (T2: brands/anchors, rule type-check, typed path
/// literal rewrite) once over the merge. Per-file parse errors carry the file's
/// path via context; merged TypeDiags attribute to the returned display path
/// (the single file, or `<dir>/*.dl` for a discovered set — per-file
/// attribution across a merge is a known coarseness). The engine never sees a
/// `Term::PathLit`: the bail guards in lower/engine are defense only.
///
/// The frontend (`frontend::load_program` / `load_program_set`) handles `use`
/// inclusion: it recursively resolves module paths against the include roots,
/// splices Core items in source order, dedups rel decls by name+cols, and
/// returns a flat core `Program`. With no `use` in any source file this is
/// byte-for-byte identical to the old parse-only path.
pub fn prepare_paths(paths: &[PathBuf]) -> Result<(ast::Program, Vec<ast::TypeDiag>, String)> {
    let items = if paths.len() == 1 {
        frontend::load_program(&paths[0])
    } else {
        frontend::load_program_set(paths)
    }?;
    let display = if paths.len() == 1 {
        paths[0].display().to_string()
    } else {
        format!("{}/*.dl", paths[0].parent().unwrap_or(Path::new(".dl")).display())
    };
    let mut prog = ast::Program { items };
    let diags = typecheck::check_and_normalize(&mut prog, &display);
    Ok((prog, diags, display))
}

/// Public so `daemon::run_daemon` can surface the same `path:line: sev[code]:
/// msg` rendering as the foreground tick path.
pub fn render_type_diags_eprintln(diags: &[ast::TypeDiag]) {
    // `coerce-text-path` is an intentional grandfather warning (a branded/path var
    // flowing into a plain text column). It re-fires every run and is pure noise on
    // a program that has accepted the coercion, so it is suppressed by default;
    // `DL_COERCE_WARN=1` opts back in. Other codes always print. Identical lines are
    // de-duped so one coercion does not print once per occurrence.
    let show_coerce = std::env::var_os("DL_COERCE_WARN").is_some();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for d in diags {
        if d.code == "coerce-text-path" && !show_coerce { continue; }
        if !seen.insert((d.code.clone(), d.msg.clone())) { continue; }
        eprintln!("{}:1: {}[{}]: {}", d.path, d.severity.as_str(), d.code, d.msg);
    }
}

pub fn run_file(program: Option<&str>, db_path: Option<&str>, root: PathBuf, query_json: bool) -> Result<()> {
    if daemon::enabled_for(&root) && db_path.is_none() {
        // Daemon owns the db; if the caller passed --db explicitly they want the
        // in-process path against that file (the daemon owns its own db).
        match run_file_via_daemon(program, &root, query_json) {
            Ok(()) => return Ok(()),
            Err(e) => {
                // Daemon path failed; fall through to in-process so a transient
                // socket error never blocks the user. Tests opt out via
                // DL_NO_DAEMON=1 and never hit this branch.
                eprintln!("[daemon] attach failed, falling back to in-process: {e}");
            }
        }
    }
    run_file_inproc(program, db_path, root, query_json)
}

/// Daemon path: ensure the daemon is up, send `query` + `diag` RPCs, render the
/// results in the same shape as `run_file_inproc` prints.
fn run_file_via_daemon(program: Option<&str>, root: &Path, query_json: bool) -> Result<()> {
    daemon::ensure_daemon(root, program)?;
    let mut s = daemon::connect(Some(root))?;
    let req = rpc::Request::new(1, "query", serde_json::json!({}));
    let resp = daemon::rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error { anyhow::bail!("daemon query: {}", e.message); }
    let results = resp.result.and_then(|v| v.get("results").cloned()).unwrap_or(serde_json::Value::Array(vec![]));
    if query_json {
        // Emit one JSON-lines object per query, same shape as --query-json.
        if let Some(arr) = results.as_array() {
            for r in arr {
                println!("{}", serde_json::to_string(r)?);
            }
        }
    } else {
        if let Some(arr) = results.as_array() {
            for r in arr {
                let rel = r.get("rel").and_then(|v| v.as_str()).unwrap_or("?");
                let cols: Vec<String> = r.get("columns").and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let rows = r.get("rows").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                println!("? {} => {}", rel, if cols.is_empty() { "(count)".into() } else { cols.join("\t") });
                for cells in rows.iter().filter_map(|r| r.as_array()) {
                    let s: Vec<String> = cells.iter().map(json_cell_tsv_render).collect();
                    println!("  {}", s.join("\t"));
                }
                println!("  ({} rows)\n", rows.len());
            }
        }
    }
    Ok(())
}

fn json_cell_tsv_render(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The pre-daemon `run_file` body, in-process. Kept for the no-daemon fallback
/// and the test path (`DL_NO_DAEMON=1`).
fn run_file_inproc(program: Option<&str>, db_path: Option<&str>, root: PathBuf, query_json: bool) -> Result<()> {
    let files = resolve_programs(program, &root)?;
    let (prog, type_diags, _) = prepare_paths(&files)?;
    render_type_diags(&type_diags, false);
    let n_errors = type_diags.iter().filter(|d| d.severity == ast::Severity::Error).count();
    if n_errors == 0 {
        let conn = db::open(db_path)?;
        let mut eng = engine::Engine::new(conn, root);
        eng.set_query_json(query_json);
        eng.set_repos(load_repos());
        // A data-driven scan (variable repo/rev) and a `repo`-sink both read
        // LAST tick's coordinate/pull state. A fresh one-shot run has no prior
        // tick, so prime once silently (derive coords / pull repos), then the
        // real run reads them and prints answers.
        let needs_prime = prog.items.iter().any(|i| matches!(i,
            ast::Item::Rule(r) if engine::scan_has_var_coords(r) || r.is_repo_sink()));
        if needs_prime {
            eng.set_prime_tick(true);
            eng.tick(&prog, true)?;
            eng.set_prime_tick(false);
        }
        eng.run(&prog)?;
    }
    if n_errors > 0 {
        anyhow::bail!("{n_errors} type error(s) in path literals / brands / stratification");
    }
    Ok(())
}

/// Render `TypeDiag`s compiler-style to stderr (`path:line: sev[code]: msg`), the
/// same shape the `diag` relation renders in `run_check`. A literal's byte span
/// maps to line 1 (the source-to-line resolver is T3 work); a zero span (a
/// structural brand/var diagnostic) also lands at line 1. `json` is reserved for
/// the `--diag-json` path which folds these into the JSON array there.
fn render_type_diags(diags: &[ast::TypeDiag], json: bool) {
    if json { return; }
    render_type_diags_eprintln(diags);
}

/// Load the turnkey config's repos, logging the source/count. A malformed
/// config is surfaced (not silently empty) so a typo never analyzes nothing.
fn load_repos() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => {
            eprintln!("[config] {} repo(s) registered (scan + type/call/doc ingestion)", cfg.repos.len());
            cfg.repos
        }
        Ok(_) => Vec::new(),
        Err(e) => { eprintln!("[config] ignored: {e}"); Vec::new() }
    }
}

/// CLI lint/ban path: run the program, render the `diag` relation, and return
/// the count of `error`-severity rows. `json` emits a JSON array to stdout for
/// hooks/CI; otherwise diags render to stderr (`path:line: sev[code]: msg`),
/// the same role as v4's LogSink. The `diag` relation is just a relation; this
/// function is one renderer of it (LSP is another). See docs/lsp.md.
///
/// Exit-code contract (enforced by main): rail violations -> 2, the Claude
/// Code blocking-hook code whose stderr feeds the agent; a broken program
/// (parse/type error) -> Err -> 1, user-facing only. A rails bug must read as
/// "fix the rails", never as agent feedback.
/// Verify-rollback codemod (christmas #14): run the program with the gen-write
/// journal armed, then run `checker` (a shell command) in `root`. Keep the edits
/// if it exits 0; otherwise restore every touched file to its pre-run bytes.
/// Returns `true` if kept (checker passed), `false` if rolled back. Always
/// in-process (never the daemon) — a verify run owns the tree transactionally.
pub fn run_verify(program: Option<&str>, db_path: Option<&str>, root: PathBuf, checker: &str)
    -> Result<bool>
{
    let files = resolve_programs(program, &root)?;
    let (prog, type_diags, _) = prepare_paths(&files)?;
    render_type_diags(&type_diags, false);
    let n_errors = type_diags.iter().filter(|d| d.severity == ast::Severity::Error).count();
    if n_errors > 0 {
        anyhow::bail!("{n_errors} type error(s) in path literals / brands / stratification");
    }
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.set_repos(load_repos());
    eng.begin_verify();
    eng.run(&prog)?;

    eprintln!("[verify] checker: {checker}");
    let status = std::process::Command::new("sh")
        .arg("-c").arg(checker)
        .current_dir(&root)
        .status()?;
    if status.success() {
        let n = eng.commit_writes();
        eprintln!("[verify] checker passed — kept {n} edited file(s)");
        Ok(true)
    } else {
        let n = eng.rollback_writes()?;
        eprintln!("[verify] checker failed (exit {}) — rolled back {n} file(s)",
            status.code().map_or("signal".into(), |c| c.to_string()));
        Ok(false)
    }
}

pub fn run_check(program: Option<&str>, db_path: Option<&str>, root: PathBuf, json: bool) -> Result<usize> {
    if daemon::enabled_for(&root) && db_path.is_none() {
        match run_check_via_daemon(program, &root, json) {
            Ok(n) => return Ok(n),
            Err(e) => eprintln!("[daemon] check attach failed, falling back to in-process: {e}"),
        }
    }
    run_check_inproc(program, db_path, root, json)
}

/// Daemon check path: ensure daemon up, send `diag` RPC, render same as
/// `run_check_inproc`. Returns the error-severity count for the exit contract.
fn run_check_via_daemon(program: Option<&str>, root: &Path, json: bool) -> Result<usize> {
    daemon::ensure_daemon(root, program)?;
    let mut s = daemon::connect(Some(root))?;
    let req = rpc::Request::new(1, "diag", serde_json::json!({}));
    let resp = daemon::rpc_call(&mut s, &req)?;
    if let Some(e) = resp.error { anyhow::bail!("daemon diag: {}", e.message); }
    let arr = resp.result.and_then(|v| v.get("rows").cloned()).and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr.clone()))?);
    } else {
        for d in &arr {
            let path = d.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let line = d.get("line").and_then(|v| v.as_i64()).unwrap_or(0);
            let sev = d.get("severity").and_then(|v| v.as_str()).unwrap_or("");
            let code = d.get("code").and_then(|v| v.as_str()).unwrap_or("");
            let msg = d.get("message").and_then(|v| v.as_str()).unwrap_or("");
            let code_s = if code.is_empty() { String::new() } else { format!("[{code}]") };
            eprintln!("{path}:{line}: {sev}{code_s}: {msg}");
            if let Some(h) = d.get("hint").and_then(|v| v.as_str()) {
                if !h.is_empty() { eprintln!("    hint: {h}"); }
            }
        }
    }
    Ok(arr.iter().filter(|d| d.get("severity").and_then(|v| v.as_str()) == Some("error")).count())
}

fn run_check_inproc(program: Option<&str>, db_path: Option<&str>, root: PathBuf, json: bool) -> Result<usize> {
    let files = resolve_programs(program, &root)?;
    // Drop `?` queries so their stdout rows don't mix with --diag-json output.
    let (mut prog, type_diags, _) = prepare_paths(&files)?;
    if json { prog.items.retain(|i| !matches!(i, ast::Item::Query(_))); }
    // `gen` never writes from a check tick: --check is the enforcement rail
    // (hooks, CI); codegen runs only on a direct `dl prog.dl` invocation.
    prog.items.retain(|i| !matches!(i, ast::Item::Gen(_)));
    // A lower-time error (brand mismatch, escaping literal, not-stratified) means
    // the program is ill-defined: skip the tick so its diagnostic, not a downstream
    // engine bail (e.g. the stratify defense) or a SQLite datatype error, surfaces.
    let type_errors = type_diags.iter().any(|d| d.severity == ast::Severity::Error);
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root);
    let diags = if type_errors { Vec::new() } else { eng.tick(&prog, true)?; eng.diags(None)? };

    if json {
        let mut arr: Vec<serde_json::Value> = diags.iter().map(|d| serde_json::json!({
            "path": d.path, "line": d.line, "col": d.col,
            "endLine": d.end_line, "endCol": d.end_col,
            "severity": d.severity, "code": d.code, "message": d.msg, "hint": d.hint,
        })).collect();
        // Fold the lower-time type diagnostics into the same JSON array (line 1;
        // span-to-line mapping is T3). The `diag`-relation rows and these share one
        // shape so a consumer treats them uniformly.
        for d in &type_diags {
            arr.push(serde_json::json!({
                "path": d.path, "line": 1, "col": d.span.0,
                "endLine": 1, "endCol": d.span.1,
                "severity": d.severity.as_str(), "code": d.code, "message": d.msg, "hint": serde_json::Value::Null,
            }));
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr))?);
    } else {
        render_type_diags(&type_diags, false);
        for d in &diags {
            let code = if d.code.is_empty() { String::new() } else { format!("[{}]", d.code) };
            eprintln!("{}:{}: {}{}: {}", d.path, d.line, d.severity, code, d.msg);
            if let Some(h) = &d.hint { eprintln!("    hint: {h}"); }
        }
    }
    let n_type = type_diags.iter().filter(|d| d.severity == ast::Severity::Error).count();
    if n_type > 0 { anyhow::bail!("{n_type} type error(s) in the program"); }
    Ok(diags.iter().filter(|d| d.severity == "error").count())
}

/// Drive one incremental tick over an existing db for a set of changed paths
/// (relative to root or absolute). The delta entry point the watcher uses.
pub fn run_changed(program: Option<&str>, db_path: Option<&str>, root: PathBuf, changed: Vec<PathBuf>) -> Result<()> {
    let files = resolve_programs(program, &root)?;
    let (prog, type_diags, _) = prepare_paths(&files)?;
    render_type_diags(&type_diags, false);
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    let abs: Vec<PathBuf> = changed.into_iter()
        .map(|p| if p.is_absolute() { p } else { root.join(p) }).collect();
    eng.tick_paths(&prog, &abs, false)
}

/// Run as an LSP server over stdio. The program's `diag` relation becomes live
/// editor diagnostics; lint fires on file open / save. See docs/lsp.md.
pub fn run_lsp(program: Option<&str>, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    lsp::run_lsp(program, db_path, root)
}

pub fn run_watch(program: Option<&str>, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    let files = resolve_programs(program, &root)?;
    let (prog, type_diags, _) = prepare_paths(&files)?;
    render_type_diags(&type_diags, false);
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.set_repos(load_repos());
    eng.tick(&prog, false)?;
    eprintln!("[watch] watching {} (ctrl-c to stop)", root.display());

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    // Watch the turnkey config file too: editing the repo list re-registers
    // repos and re-ticks. Watch its parent dir (the file may not exist yet, and
    // editors replace-on-save, which a file-level watch can miss).
    let cfg_path = config::SprfConfig::config_path().and_then(|p| p.canonicalize().ok()
        .or(Some(p)));
    if let Some(cp) = &cfg_path {
        if let Some(dir) = cp.parent() {
            if dir.exists() && watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
                eprintln!("[watch] also watching config {}", cp.display());
            }
        }
    }

    // If the program scans any git rev, watch the git dir too so a moving ref
    // (commit, checkout, branch/tag update) fires a tick even when root is a
    // subdir of the repo and `.git` is not under it.
    let scans_git = prog.items.iter().any(|i| matches!(i, ast::Item::Rule(r)
        if r.body.iter().any(|b| matches!(b, ast::BodyItem::Scan { rev: ast::Term::Str(s), .. } if s.as_str() != "WORK"))));
    let mut git_dir: Option<PathBuf> = None;
    if scans_git {
        if let Ok(out) = std::process::Command::new("git").arg("-C").arg(&root).args(["rev-parse", "--git-dir"]).output() {
            if out.status.success() {
                let gd = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let gdp = if std::path::Path::new(&gd).is_absolute() { PathBuf::from(&gd) } else { root.join(&gd) };
                if gdp.exists() && watcher.watch(&gdp, RecursiveMode::Recursive).is_ok() {
                    eprintln!("[watch] also watching refs in {}", gdp.display());
                    git_dir = gdp.canonicalize().ok();
                }
            }
        }
    }

    while let Ok(first) = rx.recv() {
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(ev) = first { paths.extend(ev.paths); }
        std::thread::sleep(std::time::Duration::from_millis(150));
        while let Ok(ev) = rx.try_recv() {
            if let Ok(ev) = ev { paths.extend(ev.paths); }
        }
        // A config edit re-registers repos; a ref move under the git dir needs
        // the full sweep (git revs); a plain file edit reconciles changed paths.
        let touches_cfg = cfg_path.as_ref().is_some_and(|c|
            paths.iter().any(|p| p.canonicalize().ok().as_deref() == Some(c) || p == c));
        let touches_git = git_dir.as_ref().is_some_and(|g| paths.iter().any(|p| p.starts_with(g)));
        if touches_cfg {
            eng.set_repos(load_repos());
            eprintln!("[watch] config changed; repos reloaded");
            eng.tick(&prog, false)?;
        } else if touches_git || paths.is_empty() {
            eng.tick(&prog, false)?;
        } else {
            eng.tick_paths(&prog, &paths, false)?;
        }
    }
    Ok(())
}

/// Auto-refactor: rewrite `use`-path references after a module move. Each `mv`
/// is `OLD_FILE=NEW_FILE` (repo-relative Rust source paths). Runs a scan-only
/// tick to populate the import graph + ref-spine, then for every located use
/// span computes the rewritten path via `rspath::rewrite_import` and splices it
/// back at the same byte coordinate. Dry-run by default (prints the planned
/// edits); `--fix` writes the files. Does NOT move the file on disk or fix the
/// moved file's own relative imports — those are separate steps.
/// Auto-refactor driver. `repo` selects which repo to rewrite: `None` = the
/// `--root` repo (self); a config slug = that repo (cloned if needed); `"*"` /
/// `"all"` = every configured repo. Each target repo is processed in isolation
/// (its own engine scanning its own root), so the use-path resolver and located
/// spans are self-correct for that repo and never cross-contaminate.
pub fn run_move(db_path: Option<&str>, root: PathBuf, repo: Option<String>, mv: Vec<String>, fix: bool) -> Result<()> {
    // Parse OLD=NEW file-move specs.
    let moves: Vec<(String, String)> = mv.iter().map(|s| {
        let (old, new) = s.split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--move expects OLD=NEW, got {s:?}"))?;
        Ok((old.trim().to_string(), new.trim().to_string()))
    }).collect::<Result<_>>()?;

    // Resolve the target repos to rewrite.
    let targets: Vec<(String, PathBuf)> = match repo.as_deref() {
        None | Some("") | Some(".") | Some("self") => vec![("self".to_string(), root.clone())],
        Some("*") | Some("all") => {
            let repos = load_repos();
            if repos.is_empty() { anyhow::bail!("--repo \"*\" needs a config with [[repos]]"); }
            repos.iter().map(|rc| {
                engine::Engine::ensure_cloned(rc)?;
                Ok((rc.slug.clone(), rc.root.clone()))
            }).collect::<Result<_>>()?
        }
        Some(slug) => {
            let repos = load_repos();
            let rc = repos.iter().find(|r| r.slug == slug)
                .ok_or_else(|| anyhow::anyhow!("--repo {slug:?} is not a configured repo slug"))?;
            engine::Engine::ensure_cloned(rc)?;
            vec![(rc.slug.clone(), rc.root.clone())]
        }
    };

    let multi = targets.len() > 1;
    for (label, troot) in targets {
        if multi { eprintln!("[move] repo {label} ({})", troot.display()); }
        // A file db is only safe for a single target; fan-out gets a transient
        // in-memory db per repo so their `_file` caches don't clobber each other.
        let conn = db::open(if multi { None } else { db_path })?;
        move_one_repo(conn, troot, &moves, fix)?;
    }
    Ok(())
}

fn move_one_repo(conn: db::Db, root: PathBuf, moves: &[(String, String)], fix: bool) -> Result<()> {
    // Scan-only source rule populates `_file` with no capture spans; referencing
    // `module_import` drives the resolver, so `_where_bytes` holds only use refs.
    let prog_src = "rel _src(p: file).\n\
        _src(p) <- scan(\"WORK\", \"**/*.rs\", p, rev).\n\
        _src(p) <- scan(\"WORK\", \"**/*.kt\", p, rev).\n\
        rel _mi(f: text, rev: text, spec: text, kind: text, ln: int).\n\
        _mi(f, rev, spec, kind, ln) <- module_import(f, rev, spec, kind, ln).\n";
    let prog = parse::parse(lex::lex(prog_src)?)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.tick(&prog, true)?;

    // Moves split by language: Rust rewrites are module-path math against
    // discovered crate roots; Kotlin rewrites are package math against the
    // moved file's own `package` declaration.
    let (kt_specs, rs_moves): (Vec<_>, Vec<_>) = moves.iter()
        .partition(|(old, _)| old.ends_with(".kt") || old.ends_with(".kts"));
    let mut kt_moves: Vec<(String, String, ktpath::KotlinMove)> = Vec::new();
    for (old, new) in &kt_specs {
        let content = std::fs::read_to_string(root.join(old))
            .map_err(|e| anyhow::anyhow!("--move {old}: cannot read the file to move: {e}"))?;
        match ktpath::plan_move(old, new, &content) {
            Ok(mv) => kt_moves.push((old.clone(), new.clone(), mv)),
            Err(e) => eprintln!("[move] {e} — its references will not be rewritten"),
        }
    }

    // Crate source roots discovered from the scanned file set (dirs holding
    // lib.rs/main.rs), so non-`src/` layouts (the kernel's rust/kernel/*.rs)
    // derive module paths instead of silently no-matching.
    let roots = rspath::crate_roots(&eng.source_paths()?);

    // Loud, not silent: a move whose endpoints resolve to no crate root at all
    // (outside `src/` AND outside any discovered root) can't be turned into a
    // module path. Say why instead of reporting a clean no-op.
    // `rs_mods` carries the derived module paths for the moves that can rewrite.
    let mut rs_mods: Vec<(String, String, String, String)> = Vec::new();
    for (old, new) in &rs_moves {
        match (rspath::file_to_mod_path_rooted(old, &roots), rspath::file_to_mod_path_rooted(new, &roots)) {
            (Some(om), Some(nm)) => rs_mods.push((old.clone(), new.clone(), om, nm)),
            _ => eprintln!("[move] cannot derive a module path for {old} or {new} \
                (under no crate root) — its references will not be rewritten"),
        }
    }

    // PASS 1 — located spans. A span in a moved file re-anchors against the
    // file's own module move (`super::` shifts, self-references follow), then
    // the other moves apply from the file's NEW module path; spans elsewhere
    // take the first move that rewrites them.
    let mut edits: Vec<refactor::Edit> = Vec::new();
    for (path, lo, hi, text) in eng.located_spans()? {
        let own = rs_mods.iter().find(|(old, _, _, _)| *old == path);
        let new_text = match own {
            Some((_, _, old_mod, new_mod)) => {
                let mut t = text.clone();
                if let Some(nt) = rspath::rewrite_moved_file_use(&t, old_mod, new_mod) { t = nt; }
                for (other_old, _, o_old_mod, o_new_mod) in &rs_mods {
                    if other_old == &path { continue; }
                    if let Some(nt) = rspath::rewrite_use_path(&t, o_old_mod, o_new_mod, new_mod) { t = nt; }
                }
                (t != text).then_some(t)
            }
            None => rs_mods.iter()
                .find_map(|(old, new, _, _)| rspath::rewrite_import_rooted(&path, old, new, &text, &roots))
                .or_else(|| kt_moves.iter().find_map(|(_, _, mv)| mv.rewrite_import(&text)))
                .filter(|nt| *nt != text),
        };
        if let Some(new_text) = new_text {
            edits.push(refactor::Edit { path, lo, hi, old_text: text, new_text });
        }
    }

    // PASS 2 — brace leaves the shared head span can't reach
    // (`use crate::{old::A, b}`): re-expand each move-relevant .rs file's use
    // statements at leaf granularity and rewrite the leaf text in place when
    // the new path still sits under the unchanged brace head. Spans already
    // edited by pass 1 (bare uses double as leaves) are skipped.
    let spanned: std::collections::HashSet<(String, u32, u32)> =
        edits.iter().map(|e| (e.path.clone(), e.lo, e.hi)).collect();
    let imports = eng.module_imports()?;
    if !rs_mods.is_empty() {
        let leaf_files: std::collections::BTreeSet<&String> = imports.iter()
            .filter(|(file, spec)| file.ends_with(".rs") && rs_mods.iter().any(|(old, new, _, _)| {
                rspath::rewrite_import_rooted(file, old, new, spec, &roots).is_some_and(|n| &n != spec)
            }))
            .map(|(file, _)| file)
            .collect();
        for file in leaf_files {
            // moved files re-anchor in pass 1; their brace leaves stay counted
            if rs_mods.iter().any(|(old, _, _, _)| old == file) { continue; }
            let Some(from_mod) = rspath::file_to_mod_path_rooted(file, &roots) else { continue };
            let Ok(content) = std::fs::read_to_string(root.join(file)) else { continue };
            for leaf in modgraph::rust_use_leaves(&content) {
                if leaf.collapsed { continue; }
                let (lo, hi) = leaf.leaf;
                if spanned.contains(&(file.clone(), lo, hi)) { continue; }
                let Some(old_text) = content.get(lo as usize..hi as usize) else { continue };
                let rewritten = rs_mods.iter().find_map(|(_, _, old_mod, new_mod)|
                    rspath::rewrite_brace_leaf(&leaf.full, &leaf.prefix, old_mod, new_mod, &from_mod));
                if let Some(new_text) = rewritten {
                    if new_text != old_text {
                        edits.push(refactor::Edit { path: file.clone(), lo, hi,
                            old_text: old_text.to_string(), new_text });
                    }
                }
            }
        }
    }

    // The moved Kotlin file's own `package` declaration follows the move.
    for (old, _, mv) in &kt_moves {
        if let Ok(content) = std::fs::read_to_string(root.join(old)) {
            if let Some((lo, hi)) = ktpath::package_decl_span(&content) {
                edits.push(refactor::Edit { path: old.clone(), lo, hi,
                    old_text: mv.old_pkg.clone(), new_text: mv.new_pkg.clone() });
            }
        }
    }

    let by_file = refactor::group_by_file(edits);

    // Honest skip accounting (file-level so brace heads don't false-positive: one
    // head edit covers all `{a, b}` leaves). A move-relevant import in a file that
    // produced NO edit is a genuine miss — e.g. a brace leaf whose rewritten path
    // left the brace head, which pass 2 can't express in place.
    let edited: std::collections::HashSet<&String> = by_file.keys().collect();
    let skipped = imports.iter().filter(|(file, spec)| {
        !edited.contains(file) && (
            rs_moves.iter().any(|(old, new)| {
                rspath::rewrite_import_rooted(file, old, new, spec, &roots).is_some_and(|n| &n != spec)
            }) || kt_moves.iter().any(|(_, _, mv)| mv.rewrite_import(spec).is_some_and(|n| &n != spec))
        )
    }).count();
    if skipped > 0 {
        eprintln!("[move] {skipped} move-relevant import(s) left alone \
            (a brace leaf whose rewrite leaves the brace head cannot be spliced in place)");
    }
    // Kotlin-specific honesty: a wildcard import of the old package may or may
    // not still cover the moved decls, and a same-package bare use breaks when
    // the file leaves the package — neither is an import-text rewrite.
    for (_, _, mv) in &kt_moves {
        let wild = imports.iter().filter(|(_, spec)| *spec == mv.old_wildcard()).count();
        if wild > 0 {
            eprintln!("[move] {wild} wildcard import(s) of {} left alone \
                (moved decls may need explicit imports of {})", mv.old_pkg, mv.new_pkg);
        }
        let bare = eng.same_package_uses()?.into_iter()
            .filter(|(_, spec)| mv.decls.iter().any(|d| d == spec)).count();
        if bare > 0 {
            eprintln!("[move] {bare} same-package bare use(s) of moved decl(s) left alone \
                (the file leaves {}; add imports of {} manually)", mv.old_pkg, mv.new_pkg);
        }
    }

    let (mut files, mut total) = (0usize, 0usize);
    for (path, file_edits) in &by_file {
        let abs = root.join(path);
        let content = std::fs::read_to_string(&abs)?;
        let rewritten = refactor::splice_file(&content, file_edits)?;
        files += 1;
        total += file_edits.len();
        for e in file_edits {
            println!("{path}: {} -> {}", e.old_text, e.new_text);
        }
        if fix {
            std::fs::write(&abs, rewritten)?;
        }
    }
    if by_file.is_empty() {
        eprintln!("[move] no use-path references to rewrite");
    } else {
        eprintln!("[move] {} edit(s) across {} file(s){}",
            total, files, if fix { ", applied" } else { " (dry run; pass --fix to apply)" });
    }

    // PASS 3 — the physical move: rename on disk, then for Rust re-home the
    // `mod` decl (remove from the old parent, declare in the new, creating the
    // parent-module chain up to an existing ancestor). Dry run prints the plan.
    for (old, new) in moves {
        if !root.join(old).is_file() {
            eprintln!("[move] {old} does not exist on disk — rename skipped");
            continue;
        }
        // a moved Rust file's child `mod x;` decls resolve relative to its
        // location and do NOT follow — say so instead of silently breaking
        if old.ends_with(".rs") {
            if let Ok(content) = std::fs::read_to_string(root.join(old)) {
                let kids = refactor::child_mod_decls(&content);
                if !kids.is_empty() {
                    eprintln!("[move] {old} declares child module(s) {kids:?} — \
                        their files do not follow the rename; move them separately");
                }
            }
        }
        println!("{old} -> {new} (rename)");
        if fix {
            if let Some(parent) = root.join(new).parent() { std::fs::create_dir_all(parent)?; }
            std::fs::rename(root.join(old), root.join(new))?;
        }
        if old.ends_with(".rs") {
            rust_mod_surgery(&root, old, new, fix)?;
        }
    }
    Ok(())
}

/// Re-home the `mod` declaration after a Rust file rename: remove
/// `mod <old_stem>;` from the old parent module file, declare `<new_stem>` in
/// the new parent, creating intermediate parent-module files (and registering
/// each in ITS parent) until an existing ancestor takes the chain. A private
/// decl leaving the crate root is promoted to `pub(crate)` so existing
/// `crate::`-path references stay visible.
fn rust_mod_surgery(root: &Path, old: &str, new: &str, fix: bool) -> Result<()> {
    let stem = |p: &str| Path::new(p).file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let (old_stem, new_stem) = (stem(old), stem(new));
    if matches!(old_stem.as_str(), "mod" | "lib" | "main") || matches!(new_stem.as_str(), "mod" | "lib" | "main") {
        eprintln!("[move] {old} or {new} is a directory-defining file (mod.rs/lib.rs/main.rs) — \
            mod-decl surgery skipped; adjust declarations manually");
        return Ok(());
    }

    // remove from the old parent
    let mut vis = String::new();
    let mut removed = false;
    for cand in rspath::mod_parent_candidates(old) {
        let abs = root.join(&cand);
        if !abs.is_file() { continue; }
        let content = std::fs::read_to_string(&abs)?;
        if let Some((without, v)) = refactor::remove_mod_decl(&content, &old_stem) {
            println!("{cand}: - {v}mod {old_stem};");
            if fix { std::fs::write(&abs, without)?; }
            vis = v;
            removed = true;
            break;
        }
    }
    if !removed {
        eprintln!("[move] no `mod {old_stem};` declaration found near {old} \
            (a #[path] decl or non-standard layout) — remove it manually");
    }

    // a private decl is crate-visible only at the crate root; relocating it
    // deeper would shrink its scope, so promote
    let is_root_file = |p: &str| matches!(stem(p).as_str(), "lib" | "main");
    let mut name = new_stem;
    let mut at = new.to_string();
    loop {
        let mut declared = false;
        for cand in rspath::mod_parent_candidates(&at) {
            let abs = root.join(&cand);
            if !abs.is_file() { continue; }
            let v = if vis.is_empty() && !is_root_file(&cand) { "pub(crate) " } else { vis.as_str() };
            let content = std::fs::read_to_string(&abs)?;
            let with = refactor::add_mod_decl(&content, &name, v);
            if with != content {
                println!("{cand}: + {v}mod {name};");
                if fix { std::fs::write(&abs, with)?; }
            }
            declared = true;
            break;
        }
        if declared { return Ok(()); }
        // no parent module file exists: create the directory's sibling file
        // (`a/b.rs` for files in `a/b/`) and register IT in its own parent
        let Some(dir) = Path::new(&at).parent().and_then(|d| d.to_str()).filter(|d| !d.is_empty()) else {
            eprintln!("[move] no parent module file found for {new} and none creatable — \
                declare `mod {name};` manually");
            return Ok(());
        };
        let parent_file = format!("{dir}.rs");
        let v = if vis.is_empty() { "pub(crate) " } else { vis.as_str() };
        println!("create {parent_file} with `{v}mod {name};`");
        if fix { std::fs::write(root.join(&parent_file), format!("{v}mod {name};\n"))?; }
        name = stem(&parent_file);
        at = parent_file;
    }
}
