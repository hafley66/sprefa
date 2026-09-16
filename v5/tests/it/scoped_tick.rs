//! Perf gap B structural proof: the FULL tick scopes its derived rebuild to
//! the rels dependency-reachable from the source relations that actually moved
//! (the same `affected_derived` walk `tick_paths` uses), instead of re-running
//! every derived statement whenever anything changed.
//! `Engine::last_derived_rebuilt` is the instrumentation.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("scoped_tick_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

// Two independent chains: src_a -> da and src_b -> db_out. Neither derived
// rule joins the built-in `file`/`repo` rels, so a change attributed to one
// chain's source must not rebuild the other chain.
const PROG: &str = r#"
rel src_a(p: file, w: text).
src_a(p, w) <- scan("WORK", "src/*.rs", p, rev), match(p, rev, /alpha_(?<w>\w+)/, line).
rel src_b(p: file, w: text).
src_b(p, w) <- scan("WORK", "src/*.txt", p, rev), match(p, rev, /beta_(?<w>\w+)/, line).
rel da(w: text).
da(w) <- src_a(_, w).
rel db_out(w: text).
db_out(w) <- src_b(_, w).
"#;

#[test]
fn full_tick_rebuilds_only_the_affected_chain() {
    let d = sandbox("chains");
    fs::write(d.join("src/a.rs"), "// alpha_one\n").unwrap();
    fs::write(d.join("src/b.txt"), "beta_one\n").unwrap();

    let prog = parse::parse(lex::lex(PROG).unwrap()).unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());

    // Cold tick: blank derived tables force the full rebuild.
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        eng.last_derived_rebuilt.len(),
        2,
        "cold tick rebuilds both chains"
    );

    // No-change tick: nothing rebuilds.
    eng.tick(&prog, true).unwrap();
    assert!(
        eng.last_derived_rebuilt.is_empty(),
        "a no-change tick rebuilds nothing, got {:?}",
        eng.last_derived_rebuilt
    );

    // Touch ONLY the b-chain's file (different byte length — the reconcile
    // (mtime secs, size) fast path can't see a same-second same-size rewrite).
    fs::write(d.join("src/b.txt"), "beta_one\nbeta_two\n").unwrap();
    eng.tick(&prog, true).unwrap();
    assert_eq!(
        eng.last_derived_rebuilt,
        vec!["db_out".to_string()],
        "an edit reaching only src_b rebuilds only its chain"
    );

    // ... and the rebuilt chain is correct while the untouched one survived.
    let db_rows = eng
        .query_sql("SELECT w FROM rel_db_out_txt ORDER BY w", &[])
        .unwrap();
    assert_eq!(db_rows.len(), 2, "db_out re-derived both beta words");
    let da_rows = eng.query_sql("SELECT w FROM rel_da_txt", &[]).unwrap();
    assert_eq!(da_rows.len(), 1, "da still holds its row untouched");
}
