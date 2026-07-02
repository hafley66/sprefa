//! Perf gap A structural proof: the type/call/dataflow extractors skip a warm
//! no-change tick entirely (the persisted `extract:*` input digest) and, when a
//! file DOES change, re-parse only the moved content (the per-file fact cache
//! keyed on (repo, path, content hash)). `Engine::extract_files_parsed` is the
//! instrumentation counter — cache misses only — so a regression back to
//! full-corpus re-parse fails here, not in a profile trace.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("extract_cache_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /fn/, line).
? type_entity(repo, sym, name, kind, parent, file, line).
? call_def(repo, sym, kind, file, line, end).
? df_node(id, kind, var, fn, file, line).
"#;

#[test]
fn warm_tick_skips_extraction_and_edit_reparses_only_the_moved_file() {
    let d = sandbox("warm");
    fs::write(d.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\npub fn alpha() -> Alpha { Alpha { n: helper() } }\nfn helper() -> u32 { 1 }\n").unwrap();
    fs::write(d.join("src/b.rs"), "pub struct Beta { pub s: String }\npub fn beta() -> Beta { Beta { s: String::new() } }\n").unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Cold tick: each family (type/call/dataflow) parses both files.
    eng.tick(&prog, true).unwrap();
    let cold = eng.extract_files_parsed.get();
    assert_eq!(cold, 6, "cold tick parses 2 files x 3 families");

    // Warm no-change tick: the extract:* digests match, no family parses.
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), cold,
        "a no-change tick must not re-parse any file");
    // ... and the rows are still served from the db (skip, not wipe).
    let n: i64 = eng.query_sql("SELECT COUNT(*) FROM rel_type_entity", &[]).unwrap().len() as i64;
    assert!(n > 0, "type_entity rows survive the skipped refresh");

    // Edit ONE file: each family re-parses only it (the other file is a
    // (path, content hash) cache hit). The new content has a different byte
    // length — reconcile's (mtime secs, size) fast path can't see a same-
    // second same-size rewrite.
    fs::write(d.join("src/a.rs"), "pub struct Alpha { pub n: u64, pub extra: bool }\npub fn alpha() -> Alpha { Alpha { n: helper(), extra: true } }\nfn helper() -> u64 { 2 }\n").unwrap();
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), cold + 3,
        "an edit re-parses the moved file once per family, not the corpus");
}

#[test]
fn fresh_process_over_a_warm_db_still_skips() {
    let d = sandbox("crossproc");
    fs::write(d.join("src/a.rs"), "pub struct Gamma { pub n: u32 }\npub fn gamma() -> u32 { 3 }\n").unwrap();
    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();

    {
        let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
        let mut eng = Engine::new(conn, d.clone());
        eng.tick(&prog, true).unwrap();
        assert!(eng.extract_files_parsed.get() > 0);
    }
    // Same db, new Engine (the one-shot rerun shape): the persisted digest
    // skips the parse even though the in-memory fact cache is cold.
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.tick(&prog, true).unwrap();
    assert_eq!(eng.extract_files_parsed.get(), 0,
        "a fresh process over an unchanged db parses nothing");
}
