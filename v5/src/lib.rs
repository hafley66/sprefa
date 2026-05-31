pub mod ast;
pub mod db;
pub mod engine;
pub mod lex;
pub mod lower;
pub mod lsp;
pub mod modgraph;
pub mod parse;
pub mod refactor;
pub mod rspath;
pub mod scc;
pub mod scip_import;
pub mod sg;
pub mod spine;
pub mod typegraph;

use anyhow::Result;
use std::path::PathBuf;

pub fn run_file(program_path: &str, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    let toks = lex::lex(&src)?;
    let prog = parse::parse(toks)?;
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root);
    eng.run(&prog)
}

/// CLI lint/ban path: run the program, render the `diag` relation, and fail the
/// command when any `error`-severity row exists. `json` emits a JSON array to
/// stdout for hooks/CI; otherwise diags render to stderr (`path:line: sev[code]:
/// msg`), the same role as v4's LogSink. The `diag` relation is just a relation;
/// this function is one renderer of it (LSP is another). See docs/lsp.md.
pub fn run_check(program_path: &str, db_path: Option<&str>, root: PathBuf, json: bool) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    // Drop `?` queries so their stdout rows don't mix with --diag-json output.
    let mut prog = parse::parse(lex::lex(&src)?)?;
    if json { prog.items.retain(|i| !matches!(i, ast::Item::Query(_))); }
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root);
    eng.tick(&prog, true)?;
    let diags = eng.diags(None)?;

    if json {
        let arr: Vec<serde_json::Value> = diags.iter().map(|d| serde_json::json!({
            "path": d.path, "line": d.line, "col": d.col,
            "endLine": d.end_line, "endCol": d.end_col,
            "severity": d.severity, "code": d.code, "message": d.msg, "hint": d.hint,
        })).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::Value::Array(arr))?);
    } else {
        for d in &diags {
            let code = if d.code.is_empty() { String::new() } else { format!("[{}]", d.code) };
            eprintln!("{}:{}: {}{}: {}", d.path, d.line, d.severity, code, d.msg);
            if let Some(h) = &d.hint { eprintln!("    hint: {h}"); }
        }
    }
    let errors = diags.iter().filter(|d| d.severity == "error").count();
    if errors > 0 { anyhow::bail!("{errors} banned pattern(s) found"); }
    Ok(())
}

/// Drive one incremental tick over an existing db for a set of changed paths
/// (relative to root or absolute). The delta entry point the watcher uses.
pub fn run_changed(program_path: &str, db_path: Option<&str>, root: PathBuf, changed: Vec<PathBuf>) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    let prog = parse::parse(lex::lex(&src)?)?;
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    let abs: Vec<PathBuf> = changed.into_iter()
        .map(|p| if p.is_absolute() { p } else { root.join(p) }).collect();
    eng.tick_paths(&prog, &abs, false)
}

/// Run as an LSP server over stdio. The program's `diag` relation becomes live
/// editor diagnostics; lint fires on file open / save. See docs/lsp.md.
pub fn run_lsp(program_path: &str, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    lsp::run_lsp(program_path, db_path, root)
}

pub fn run_watch(program_path: &str, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    use notify::{RecursiveMode, Watcher};
    let src = std::fs::read_to_string(program_path)?;
    let prog = parse::parse(lex::lex(&src)?)?;
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.tick(&prog, false)?;
    eprintln!("[watch] watching {} (ctrl-c to stop)", root.display());

    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res| { let _ = tx.send(res); })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

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
        // A ref move under the git dir needs the full sweep (git revs); a plain
        // file edit reconciles only the changed paths.
        let touches_git = git_dir.as_ref().is_some_and(|g| paths.iter().any(|p| p.starts_with(g)));
        if touches_git || paths.is_empty() {
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
pub fn run_move(db_path: Option<&str>, root: PathBuf, mv: Vec<String>, fix: bool) -> Result<()> {
    // Parse OLD=NEW file-move specs.
    let moves: Vec<(String, String)> = mv.iter().map(|s| {
        let (old, new) = s.split_once('=')
            .ok_or_else(|| anyhow::anyhow!("--move expects OLD=NEW, got {s:?}"))?;
        Ok((old.trim().to_string(), new.trim().to_string()))
    }).collect::<Result<_>>()?;

    // Scan-only source rule populates `_file` with no capture spans; referencing
    // `module_import` drives the resolver, so `_where_bytes` holds only use refs.
    let prog_src = "rel _src(p: file).\n\
        _src(p) <- scan(\"WORK\", \"**/*.rs\", p, rev).\n\
        rel _mi(f: text, rev: text, spec: text, kind: text, ln: int).\n\
        _mi(f, rev, spec, kind, ln) <- module_import(f, rev, spec, kind, ln).\n";
    let prog = parse::parse(lex::lex(prog_src)?)?;
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.tick(&prog, true)?;

    // Each located use span -> the first move that rewrites it.
    let mut edits: Vec<refactor::Edit> = Vec::new();
    for (path, lo, hi, text) in eng.located_spans()? {
        for (old, new) in &moves {
            if let Some(new_text) = rspath::rewrite_import(&path, old, new, &text) {
                if new_text != text {
                    edits.push(refactor::Edit { path, lo, hi, old_text: text, new_text });
                    break;
                }
            }
        }
    }

    // Honest skip accounting: a brace leaf (`use a::{b, c}`) produces a
    // module_import per leaf but its located span covers the leaf name, not the
    // full path, so it has no clean rewrite coordinate yet. Count specifiers a
    // move would change but that produced no edit, and say so.
    let would_rewrite = eng.module_imports()?.iter().filter(|(file, spec)| {
        moves.iter().any(|(old, new)| {
            rspath::rewrite_import(file, old, new, spec).is_some_and(|n| &n != spec)
        })
    }).count();
    let skipped = would_rewrite.saturating_sub(edits.len());
    if skipped > 0 {
        eprintln!("[move] {skipped} brace-import reference(s) not rewritten \
            (brace head-span pending; see CLAUDE.md F1b)");
    }

    if edits.is_empty() {
        eprintln!("[move] no use-path references to rewrite");
        return Ok(());
    }
    let by_file = refactor::group_by_file(edits);
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
    eprintln!("[move] {} edit(s) across {} file(s){}",
        total, files, if fix { ", applied" } else { " (dry run; pass --fix to apply)" });
    Ok(())
}
