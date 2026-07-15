//! Small generated-corpus probe for the call-graph reactive path.
//! No CLI, daemon, watcher, config, repo discovery, or effects.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sprefa_v5::engine::{Engine, PathTickFallbackPolicy, PathTickOutcome};
use sprefa_v5::{db, lex, parse};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const PROGRAM: &str = r#"
rel seen(path: file).
seen(path) <- scan("WORK", "corpus/*.rs", path, rev), match(path, rev, /pub fn/, line).

? call_def(repo, sym, kind, file, line, end).
? call_name(sym, name).
? call_edge(caller, callee, kind).
"#;

fn main() {
    if let Err(error) = run() {
        eprintln!("reactivity probe: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let (root, database, changed) = args()?;
    rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build_global()
        .context("Rayon was initialized before the two-worker probe")?;
    if rayon::current_num_threads() != 2 {
        bail!("expected two Rayon workers");
    }

    let program = parse::parse(lex::lex(PROGRAM)?).context("parse fixed call-graph program")?;
    refuse_database(&database)?;
    let connection = db::open(Some(path_text(&database)?))?;
    let mut engine = Engine::new(connection, root.clone());

    let cold = full_tick(&mut engine, &program)?;
    let warm = full_tick(&mut engine, &program)?;
    let before_edit = fs::read_to_string(&changed)?;
    if !before_edit.contains("helper_a(value)") {
        bail!("edit target does not contain the expected helper_a call");
    }
    fs::write(&changed, before_edit.replacen("helper_a(value)", "helper_b(value)", 1))?;

    let before_parses = engine.extract_files_parsed.get();
    let started = Instant::now();
    let outcome = engine.tick_paths_with_policy(
        &program,
        std::slice::from_ref(&changed),
        true,
        PathTickFallbackPolicy::Forbid,
    )?;
    if outcome != PathTickOutcome::Incremental {
        bail!("one-file edit did not stay incremental: {outcome:?}");
    }
    let edit = phase(
        started,
        engine.extract_files_parsed.get().saturating_sub(before_parses),
        &engine,
        "incremental",
    )?;
    drop(engine);

    let clean_database = database.with_extension("clean.sqlite");
    refuse_database(&clean_database)?;
    let connection = db::open(Some(path_text(&clean_database)?))?;
    let mut clean_engine = Engine::new(connection, root);
    let rebuild = full_tick(&mut clean_engine, &program)?;
    if edit["semantic_digest"] != rebuild["semantic_digest"]
        || edit["rows"] != rebuild["rows"]
    {
        bail!("incremental call graph differs from a clean rebuild");
    }

    println!(
        "{}",
        serde_json::to_string(&json!({
            "schema_version": 1,
            "fixture_files": fixture_file_count(&changed)?,
            "workers": 2,
            "cold": cold,
            "warm": warm,
            "one_file_edit": edit,
            "clean_rebuild": rebuild,
            "equivalent": true,
        }))?
    );
    Ok(())
}

fn args() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let mut args = env::args_os().skip(1).map(PathBuf::from);
    let root = args.next().context("usage: reactivity_probe ROOT DB CHANGED_FILE")?;
    let database = args.next().context("usage: reactivity_probe ROOT DB CHANGED_FILE")?;
    let changed = args.next().context("usage: reactivity_probe ROOT DB CHANGED_FILE")?;
    if args.next().is_some() {
        bail!("usage: reactivity_probe ROOT DB CHANGED_FILE");
    }
    let root = root.canonicalize()?;
    let changed = changed.canonicalize()?;
    if !changed.starts_with(&root) {
        bail!("changed file is outside the fixture root");
    }
    let database = absolute_new_path(&database)?;
    if !database.starts_with(env!("CARGO_MANIFEST_DIR")) {
        bail!("database must be inside the sprefa repository");
    }
    Ok((root, database, changed))
}

fn full_tick(engine: &mut Engine, program: &sprefa_v5::ast::Program) -> Result<Value> {
    let before_parses = engine.extract_files_parsed.get();
    let started = Instant::now();
    engine.tick(program, true)?;
    phase(
        started,
        engine.extract_files_parsed.get().saturating_sub(before_parses),
        engine,
        "full",
    )
}

fn phase(started: Instant, files_parsed: usize, engine: &Engine, kind: &str) -> Result<Value> {
    let rows = call_graph_rows(engine)?;
    let encoded = serde_json::to_vec(&rows)?;
    Ok(json!({
        "kind": kind,
        "wall_ms": started.elapsed().as_secs_f64() * 1000.0,
        "files_parsed": files_parsed,
        "rows": rows.len(),
        "semantic_digest": blake3::hash(&encoded).to_hex().to_string(),
    }))
}

fn call_graph_rows(engine: &Engine) -> Result<Vec<String>> {
    let mut rows = Vec::new();
    for table in ["rel_call_def_txt", "rel_call_name_txt", "rel_call_edge_txt"] {
        for row in engine.query_sql(&format!("SELECT * FROM {table}"), &[])? {
            rows.push(format!("{table}:{}", serde_json::to_string(&row)?));
        }
    }
    rows.sort_unstable();
    Ok(rows)
}

fn fixture_file_count(changed: &Path) -> Result<usize> {
    let corpus = changed.parent().context("changed file has no corpus directory")?;
    Ok(fs::read_dir(corpus)?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .count())
}

fn absolute_new_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        bail!("database already exists: {}", path.display());
    }
    let parent = path.parent().context("database has no parent")?.canonicalize()?;
    Ok(parent.join(path.file_name().context("database has no filename")?))
}

fn refuse_database(path: &Path) -> Result<()> {
    for suffix in ["", "-wal", "-shm"] {
        let candidate = PathBuf::from(format!("{}{suffix}", path.display()));
        if candidate.exists() {
            bail!("database artifact already exists: {}", candidate.display());
        }
    }
    Ok(())
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str().context("database path is not UTF-8")
}
