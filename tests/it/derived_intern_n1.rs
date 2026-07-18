//! N+1 structural guard for the DERIVED fixpoint's string interning.
//!
//! A derived `INSERT ... SELECT` whose head materializes a NEW string (a literal
//! head or a text `+` concat) interns it through the `sprf_sym_intern` SQL
//! scalar, which queues the (id, text) pair on the engine-global pending-sym
//! accumulator. `rebuild_derived` used to `flush_pending_syms` after EVERY such
//! statement, so a rebuild running many minting statements flushed many times —
//! `[n+1] 'INSERT _strings' ran Nx this tick`. The flush is now hoisted to each
//! fixpoint PASS boundary (once per pass in `run_component` /
//! `rebuild_derived_seminaive`), which is the minimal frequency correctness
//! needs: semi-naive/naive read the prior pass, so a pass's minted strings must
//! be visible before the next pass decodes them, but no more often.
//!
//! `many_minting_rules_one_pass_has_no_string_n1`: one non-recursive rel headed
//! by 80 rules, each minting a distinct literal string. That is one dependency
//! component evaluated in a single pass. Pre-fix it flushed 80 times (one per
//! statement, over the 64 threshold); post-fix it is a single flush after the
//! pass. Structural, read off `Engine::last_n1`, not a timing vibe.

use sprefa_v5::{db, engine::Engine, lex, parse};
use std::fs;
use std::path::PathBuf;

const MINTING_RULES: usize = 80;

// One non-recursive rel `label`, headed by MINTING_RULES rules, each producing a
// distinct freshly-interned string from a single seed fact. All 80 rules land in
// the one `label` component and lower to 80 INSERTs run in a single pass — 80
// per-statement flushes under the old code, one pass-boundary flush now.
fn minting_program() -> String {
    let mut prog = String::from("rel seed(n: int).\nseed(1).\nrel label(s: text).\n");
    for k in 0..MINTING_RULES {
        prog.push_str(&format!("label(\"interned_symbol_{k}\") <- seed(n).\n"));
    }
    prog.push_str("? label(s).\n");
    prog
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("derived_intern_n1_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn many_minting_rules_one_pass_has_no_string_n1() {
    let dir = sandbox("minting");
    let prog = parse::parse(lex::lex(&minting_program()).unwrap()).unwrap();
    let conn = db::open(Some(dir.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, dir.clone());
    eng.tick(&prog, true).unwrap();

    // Every rule fired: 80 distinct interned strings landed in `label`. This is
    // the row count that, at one-flush-per-statement, would blow past the N1
    // threshold (64).
    let rows = eng.count_rows("label").unwrap_or(-1);
    assert_eq!(
        rows, MINTING_RULES as i64,
        "each minting rule must land one row (got {rows})"
    );
    assert!(
        rows > 64,
        "fixture must mint enough strings in one pass to expose the N+1 (got {rows})"
    );

    // The structural guarantee: minting 80 strings in a single derived pass must
    // NOT trip the per-tick N+1 detector. Pre-fix this fired
    //   [n+1] 'INSERT _strings' ran 80x this tick
    // because `rebuild_derived` flushed the intern queue after every statement.
    assert!(
        eng.last_n1.is_none(),
        "derived string interning tripped the N+1 detector: {:?} — a per-statement \
         flush_pending_syms slipped back into the fixpoint",
        eng.last_n1
    );
}
