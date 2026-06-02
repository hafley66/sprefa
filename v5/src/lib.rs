pub mod ast;
pub mod config;
pub mod db;
pub mod engine;
pub mod lex;
pub mod lower;
pub mod lsp;
pub mod modgraph;
pub mod parse;
pub mod refactor;
pub mod repo;
pub mod rspath;
pub mod scc;
pub mod scip_import;
pub mod sg;
pub mod spine;
pub mod typegraph;

use anyhow::Result;
use std::path::PathBuf;

pub fn run_file(program_path: &str, db_path: Option<&str>, root: PathBuf, query_json: bool) -> Result<()> {
    let src = std::fs::read_to_string(program_path)?;
    let toks = lex::lex(&src)?;
    let prog = parse::parse(toks)?;
    let conn = db::open(db_path)?;
    let mut eng = engine::Engine::new(conn, root);
    eng.set_query_json(query_json);
    eng.set_repos(load_repos());
    eng.run(&prog)
}

/// Load the turnkey config's repos, logging the source/count. A malformed
/// config is surfaced (not silently empty) so a typo never analyzes nothing.
fn load_repos() -> Vec<config::RepoConfig> {
    match config::SprfConfig::load_default() {
        Ok(cfg) if !cfg.repos.is_empty() => {
            eprintln!("[config] {} repo(s) registered (file ingestion: --root only so far)", cfg.repos.len());
            cfg.repos
        }
        Ok(_) => Vec::new(),
        Err(e) => { eprintln!("[config] ignored: {e}"); Vec::new() }
    }
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
        rel _mi(f: text, rev: text, spec: text, kind: text, ln: int).\n\
        _mi(f, rev, spec, kind, ln) <- module_import(f, rev, spec, kind, ln).\n";
    let prog = parse::parse(lex::lex(prog_src)?)?;
    let mut eng = engine::Engine::new(conn, root.clone());
    eng.tick(&prog, true)?;

    // Crate source roots discovered from the scanned file set (dirs holding
    // lib.rs/main.rs), so non-`src/` layouts (the kernel's rust/kernel/*.rs)
    // derive module paths instead of silently no-matching.
    let roots = rspath::crate_roots(&eng.source_paths()?);

    // Loud, not silent: a move whose endpoints resolve to no crate root at all
    // (outside `src/` AND outside any discovered root) can't be turned into a
    // module path. Say why instead of reporting a clean no-op.
    for (old, new) in moves {
        if rspath::file_to_mod_path_rooted(old, &roots).is_none()
            || rspath::file_to_mod_path_rooted(new, &roots).is_none()
        {
            eprintln!("[move] cannot derive a module path for {old} or {new} \
                (under no crate root) — its references will not be rewritten");
        }
    }

    // Each located use span -> the first move that rewrites it.
    let mut edits: Vec<refactor::Edit> = Vec::new();
    for (path, lo, hi, text) in eng.located_spans()? {
        for (old, new) in moves {
            if let Some(new_text) = rspath::rewrite_import_rooted(&path, old, new, &text, &roots) {
                if new_text != text {
                    edits.push(refactor::Edit { path, lo, hi, old_text: text, new_text });
                    break;
                }
            }
        }
    }

    let by_file = refactor::group_by_file(edits);

    // Honest skip accounting (file-level so brace heads don't false-positive: one
    // head edit covers all `{a, b}` leaves). A move-relevant import in a file that
    // produced NO edit is a genuine miss — e.g. `use crate::{old::A, ..}` whose
    // head prefix isn't the moved module, so the head span doesn't match.
    let edited: std::collections::HashSet<&String> = by_file.keys().collect();
    let skipped = eng.module_imports()?.into_iter().filter(|(file, spec)| {
        !edited.contains(file) && moves.iter().any(|(old, new)| {
            rspath::rewrite_import_rooted(file, old, new, spec, &roots).is_some_and(|n| &n != spec)
        })
    }).count();
    if skipped > 0 {
        eprintln!("[move] {skipped} move-relevant import(s) left alone \
            (head prefix not the moved module, e.g. `use crate::{{old::A, ..}}`)");
    }

    if by_file.is_empty() {
        eprintln!("[move] no use-path references to rewrite");
        return Ok(());
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
    eprintln!("[move] {} edit(s) across {} file(s){}",
        total, files, if fix { ", applied" } else { " (dry run; pass --fix to apply)" });
    Ok(())
}
