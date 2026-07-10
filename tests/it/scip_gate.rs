//! The scip family must load for call/type programs that never NAME a scip
//! rel, and the load must precede extraction within the tick. Regression for
//! the two stacked bugs the otel corpus measurement surfaced (2026-07-10):
//! (1) `ScipKind::used` defaulted to "program names a scip rel", so
//! `SPREFA_SCIP_INDEX` was a silent no-op for exactly the call/type programs
//! it should improve; (2) the index loaded in the RelKind loop AFTER the
//! extract families, so even a forced load only reached the resolvers on
//! tick 2 via the digest fold.

use std::fs;
use std::path::{Path, PathBuf};

use scip::types::{Document, Index, Occurrence, SymbolRole};

use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_scipgate_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn occurrence(symbol: &str, roles: i32) -> Occurrence {
    let mut occ = Occurrence::new();
    occ.symbol = symbol.to_string();
    occ.symbol_roles = roles;
    occ
}

fn document(path: &str, occurrences: Vec<Occurrence>) -> Document {
    let mut doc = Document::new();
    doc.relative_path = path.to_string();
    doc.occurrences = occurrences;
    doc
}

fn write_index(path: &Path, docs: Vec<Document>) {
    let mut index = Index::new();
    index.documents = docs;
    scip::write_message_to_file(path, index).unwrap();
}

/// descriptor name parses to `helper`, matching the call site's text.
const SYM: &str = "scip-go gomod fixture 1.0.0 `main`/helper().";

/// Two same-package defs of `helper` make the syntactic tier's ambiguity
/// bucket a tie (stays bare); only the index knows the real def file.
fn write_fixture(root: &Path) {
    fs::write(root.join("a.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(root.join("b.go"), "package main\n\nfunc helper() {}\n").unwrap();
    fs::write(
        root.join("caller.go"),
        "package main\n\nfunc caller() {\n\thelper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("p.dl"),
        "rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"**/*.go\", path, rev).\n\
         rel edge(caller_sym: text, callee_sym: text).\n\
         edge(caller_sym, callee_sym) <- call_edge(caller_sym, callee_sym, kind).\n\
         rel site(caller_sym: text, callee_name: text).\n\
         site(caller_sym, callee_name) <- call_site(repo, caller_sym, callee_name, path, line).\n",
    )
    .unwrap();
}

/// One tick; returns (edge rows, site rows).
fn graph_after_one_tick(root: &Path) -> (Vec<Vec<String>>, Vec<Vec<String>>) {
    let conn = db::open(Some(root.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root.to_path_buf());
    let (prog, diags, _) = prepare_paths(&[root.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "program should typecheck: {diags:?}");
    eng.tick(&prog, true).unwrap();
    (eng.rel_rows("edge", 2), eng.rel_rows("site", 2))
}

/// Control: with no index the two-def tie stays bare, proving the fixture is
/// genuinely undecidable for the syntactic tier (the positive twin below is
/// not passing by accident of name resolution).
#[test]
fn ambiguous_call_stays_bare_without_index() {
    let d = sandbox("bare");
    write_fixture(&d);
    let (edges, sites) = graph_after_one_tick(&d);
    assert!(
        sites
            .iter()
            .any(|r| r[0].contains("caller.go") && r[1] == "helper"),
        "the call site itself must exist: {sites:?}"
    );
    assert!(
        !edges.iter().any(|r| r[0].contains("caller.go")),
        "two same-dir defs must stay unresolved (no edge) syntactically: {edges:?}"
    );
}

/// The real assertion: a root index.scip resolves the call on the FIRST tick
/// of a program that names no scip rel.
#[test]
fn index_resolves_call_first_tick_without_naming_scip_rels() {
    let d = sandbox("first");
    write_fixture(&d);
    write_index(
        &d.join("index.scip"),
        vec![
            document("b.go", vec![occurrence(SYM, SymbolRole::Definition as i32)]),
            document("caller.go", vec![occurrence(SYM, 0)]),
        ],
    );
    let (edges, _) = graph_after_one_tick(&d);
    let callee = &edges
        .iter()
        .find(|r| r[0].contains("caller.go"))
        .unwrap_or_else(|| panic!("no edge from caller.go: {edges:?}"))[1];
    assert!(
        callee.contains("b.go::"),
        "first tick must resolve via the index to b.go, got {callee}"
    );
}
