//! Watch salsa behave, on the real crate.
//!
//!   cargo run --example salsa_behaves
//!
//! Three source files feed a two-level derived graph:
//!   file.text ──▶ edge_count(file) ──▶ total_edges(all files)
//! We edit ONE file and read the event log: which queries EXECUTED (body ran) vs
//! were VALIDATED (reused from memo). Then we edit a file so its derived value is
//! UNCHANGED and show the early-cutoff — the top query does not re-run.

use reactor_lab::Db;
use salsa::Setter;

#[salsa::input]
struct SourceFile {
    name: String,
    text: String,
}

/// Derived, level 1: count `caller callee` lines in one file. Depends only on that
/// file's text — salsa records that dependency automatically when we read `.text`.
#[salsa::tracked]
fn edge_count(db: &dyn salsa::Database, file: SourceFile) -> usize {
    file.text(db).lines().filter(|l| l.split_whitespace().count() == 2).count()
}

/// Derived, level 2: sum the level-1 counts. Depends on all three level-1 queries.
#[salsa::tracked]
fn total_edges(db: &dyn salsa::Database, files: Files) -> usize {
    files.list(db).iter().map(|f| edge_count(db, *f)).sum()
}

#[salsa::input]
struct Files {
    list: Vec<SourceFile>,
}

fn step(db: &Db, files: Files, label: &str) {
    let total = total_edges(db, files);
    let (exec, val, lines) = db.drain();
    println!("\n[{label}]  total_edges = {total}   (EXECUTE {exec}, VALIDATE {val})");
    for l in lines {
        println!("    {l}");
    }
}

fn main() {
    let db = Db::default();
    let a = SourceFile::new(&db, "a.rs".into(), "main parse\nmain lower".into()); // 2
    let b = SourceFile::new(&db, "b.rs".into(), "parse lex".into()); // 1
    let c = SourceFile::new(&db, "c.rs".into(), "handler validate\nvalidate main".into()); // 2
    let files = Files::new(&db, vec![a, b, c]);

    // cold: everything must execute (5 queries: 3 edge_count + total + the read).
    step(&db, files, "cold build");

    // re-ask with NOTHING changed: everything validated from memo, nothing executes.
    step(&db, files, "re-query, no change");

    // edit ONLY b: its text goes 1 -> 3 edges. Expect: edge_count(b) EXECUTES,
    // edge_count(a) and (c) VALIDATE (untouched), total_edges EXECUTES (a dep moved).
    let mut db = db;
    b.set_text(&mut db).to("parse lex\nlex read\nread emit".into());
    step(&db, files, "edit b (1 -> 3 edges): only b's branch re-runs");

    // edit a so edge_count(a) recomputes to the SAME value (2 edges, different text).
    // Expect: edge_count(a) EXECUTES, but total_edges does NOT — backdating cuts the
    // wave because a's derived output did not move.
    a.set_text(&mut db).to("main PARSE\nmain LOWER".into()); // still 2 edges
    step(&db, files, "edit a but SAME edge_count -> early cutoff, total does NOT re-run");
}
