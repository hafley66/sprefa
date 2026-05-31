pub mod ast;
pub mod db;
pub mod engine;
pub mod lex;
pub mod lower;
pub mod lsp;
pub mod modgraph;
pub mod parse;
pub mod scc;
pub mod sg;
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
