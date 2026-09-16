//! Lazy multi-repo SCIP via the user-derived `scip_want(repo)` demand rel: the
//! self index plus each wanted repo's index are each loaded independently and
//! their rows tagged with the origin repo. Each index resolves refs WITHIN its
//! own document set (cross-repo SCIP resolution is intentionally dropped — two
//! roots of the same crate would otherwise collapse onto one def). Drives the
//! engine in-proc over two ticks (want derives on tick 1, the wanted repo's
//! index lands on tick 2 — the data-driven-scan latency contract).

use std::fs;
use std::path::{Path, PathBuf};

use scip::types::{Document, Index, Occurrence, SymbolRole};

use sprefa_v5::config::RepoConfig;
use sprefa_v5::db;
use sprefa_v5::engine::Engine;
use sprefa_v5::prepare_paths;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_scipwant_{tag}_{}", std::process::id()));
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

/// The shared symbol. Both indexes are self-contained (each has a def AND a ref
/// of it) so each resolves its own ref; the demand rel proves the second repo's
/// index is loaded and tagged distinctly, not merged into the first.
const SYM: &str = "scip-go gomod lib 1.0.0 `lib`/Answer().";

#[test]
fn scip_want_loads_wanted_repo_indexes_per_repo() {
    let d = sandbox("merge");
    let dep = d.join("dep-repo");
    fs::create_dir_all(&dep).unwrap();
    // self index: app/lib.go DEFINES it, app/main.go REFERENCES it (self-contained).
    write_index(
        &d.join("index.scip"),
        vec![
            document(
                "app/lib.go",
                vec![occurrence(SYM, SymbolRole::Definition as i32)],
            ),
            document("app/main.go", vec![occurrence(SYM, 0)]),
        ],
    );
    // dep index: lib/lib.go DEFINES it, lib/main.go REFERENCES it (self-contained).
    write_index(
        &dep.join("index.scip"),
        vec![
            document(
                "lib/lib.go",
                vec![occurrence(SYM, SymbolRole::Definition as i32)],
            ),
            document("lib/main.go", vec![occurrence(SYM, 0)]),
        ],
    );

    fs::write(
        d.join("p.dl"),
        "scip_want(\"dep\").\n\
         rel r(file: text, symbol: text, def_file: text, repo: text).\n\
         r(F, S, D, R) <- scip_ref(F, S, D, R).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![RepoConfig {
        slug: "dep".into(),
        root: dep.clone(),
        url: None,
        allow_missing: false,
    }]);
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "scip_want program should typecheck: {diags:?}");

    // Tick 1: want is empty at refresh time — only the self index loads. Its ref
    // resolves WITHIN itself (app/main.go -> app/lib.go); no dep-tagged rows yet.
    eng.tick(&prog, true).unwrap();
    let rows = eng.rel_rows("r", 4);
    assert!(
        rows.iter()
            .any(|r| r[0] == "app/main.go" && r[2] == "app/lib.go"),
        "tick 1 self ref resolves within the self index: {rows:?}"
    );
    assert!(
        !rows.iter().any(|r| r[3] == "dep"),
        "tick 1 must not load the dep index yet: {rows:?}"
    );

    // Tick 2: the want row demands dep; its index loads as a SEPARATE input,
    // tagged repo="dep", resolving its own ref within itself — NOT merged.
    eng.tick(&prog, true).unwrap();
    let rows = eng.rel_rows("r", 4);
    let dep_row = rows
        .iter()
        .find(|r| r[3] == "dep")
        .unwrap_or_else(|| panic!("no dep-tagged ref row on tick 2: {rows:?}"));
    assert_eq!(dep_row[0], "lib/main.go");
    assert_eq!(dep_row[1], SYM);
    assert_eq!(
        dep_row[2], "lib/lib.go",
        "dep ref resolves within the dep index, not across repos: {rows:?}"
    );
    // both repos' rows coexist with distinct repo tags (no cross-root collapse).
    assert!(
        rows.iter().any(|r| r[0] == "app/main.go" && r[3] != "dep"),
        "self ref row still present, distinct repo tag: {rows:?}"
    );
}

/// Tick accounting for the scip_want demand chain when the consumer is the CALL
/// EXTRACT family (not a plain derived rule). The `pre_extract` ordering fix
/// (commit 84ae5d7) runs the scip index load BEFORE the type/call extract
/// families within a tick, so the resolver reads the freshly-loaded index the
/// SAME tick it lands. Composed with the one-tick demand latency (`scip_want`
/// derives on tick 1, so the wanted index is only demanded on tick 2), a wanted
/// repo's call resolves on TICK 2 — cut from the pre-fix TICK 3 (extract ran
/// index-blind, healing only on the next tick's digest fold).
///
/// Fixture: the wanted repo `dep` has TWO same-package defs of `Answer`
/// (syntactically a tie, stays bare) plus a `caller` that calls it; the dep
/// index names the ONE real def. Tick 1: `scip_want` empty at load time -> no dep
/// index -> the call is bare. Tick 2: the want row demands dep, its index loads
/// pre-extract, and the call resolves to the indexed def in the SAME tick.
#[test]
fn scip_want_call_resolution_lands_on_tick_two() {
    const ANSWER: &str = "scip-go gomod lib 1.0.0 `lib`/Answer().";
    let d = sandbox("callchain");
    let dep = d.join("dep-repo");
    fs::create_dir_all(&dep).unwrap();
    // self root: one trivial file so the program has something to scan.
    fs::write(d.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
    // dep repo: two same-package Answer defs (a syntactic tie) + a caller.
    fs::write(
        dep.join("a.go"),
        "package lib\n\nfunc Answer() int { return 1 }\n",
    )
    .unwrap();
    fs::write(
        dep.join("b.go"),
        "package lib\n\nfunc Answer() int { return 2 }\n",
    )
    .unwrap();
    fs::write(
        dep.join("caller.go"),
        "package lib\n\nfunc caller() {\n\tAnswer()\n}\n",
    )
    .unwrap();
    // dep index: a.go DEFINES Answer, caller.go REFERENCES it -> the name-level
    // map resolves (dep, caller.go, "Answer") -> a.go, disambiguating the tie.
    write_index(
        &dep.join("index.scip"),
        vec![
            document(
                "a.go",
                vec![occurrence(ANSWER, SymbolRole::Definition as i32)],
            ),
            document("caller.go", vec![occurrence(ANSWER, 0)]),
        ],
    );

    fs::write(
        d.join("p.dl"),
        "scip_want(\"dep\").\n\
         rel seen(path: file).\n\
         seen(path) <- scan(\"WORK\", \"**/*.go\", path, rev).\n\
         seen(path) <- scan(\"*\", \"WORK\", \"**/*.go\", path, rev).\n\
         rel edge(caller: text, callee: text).\n\
         edge(caller, callee) <- call_edge(caller, callee, kind).\n\
         rel site(callee_name: text, path: file).\n\
         site(callee_name, path) <- call_site(repo, caller, callee_name, path, line).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![RepoConfig {
        slug: "dep".into(),
        root: dep.clone(),
        url: None,
        allow_missing: false,
    }]);
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags
        .iter()
        .filter(|x| x.severity == sprefa_v5::ast::Severity::Error)
        .count();
    assert_eq!(errs, 0, "program should typecheck: {diags:?}");

    // Tick 1: scip_want is empty at pre-extract time -> the dep index does not
    // load -> the two-def `Answer` call stays a syntactic tie (bare, no edge).
    // The call SITE must exist though, so the bareness is genuine ambiguity (dep
    // IS scanned), not the dep files simply being absent from the corpus.
    eng.tick(&prog, true).unwrap();
    let sites1 = eng.rel_rows("site", 2);
    assert!(
        sites1
            .iter()
            .any(|r| r[0] == "Answer" && r[1].contains("caller.go")),
        "tick 1: dep must be scanned (the Answer call site exists): {sites1:?}"
    );
    let edges1 = eng.rel_rows("edge", 2);
    assert!(
        !edges1.iter().any(|r| r[1].contains("Answer")),
        "tick 1: no dep index yet, the two-def Answer call must be bare: {edges1:?}"
    );

    // Tick 2: the want row (derived on tick 1) demands dep; its index loads
    // pre-extract and the CALL family consumes it the SAME tick, resolving the
    // call to the indexed def a.go. Landing here (not tick 3) is the pre_extract
    // ordering payoff.
    eng.tick(&prog, true).unwrap();
    let edges2 = eng.rel_rows("edge", 2);
    assert!(edges2.iter().any(|r| r[1].contains("a.go::") && r[1].contains("Answer")),
        "tick 2: dep index loads pre-extract and the call resolves to a.go the same tick: {edges2:?}");
}

/// A wanted repo with no index and no installed indexer for its (absent)
/// markers skips loudly; the self index still loads alone.
#[test]
fn scip_want_skips_unindexable_repo() {
    let d = sandbox("skip");
    let dep = d.join("bare-repo");
    fs::create_dir_all(&dep).unwrap(); // no markers, no index
    write_index(
        &d.join("index.scip"),
        vec![document(
            "app/main.go",
            vec![occurrence(SYM, SymbolRole::Definition as i32)],
        )],
    );

    fs::write(
        d.join("p.dl"),
        "scip_want(\"bare\").\n\
         scip_want(\"no-such-slug\").\n\
         rel r(symbol: text, file: text).\n\
         r(S, F) <- scip_def(S, F, _).\n",
    )
    .unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![RepoConfig {
        slug: "bare".into(),
        root: dep.clone(),
        url: None,
        allow_missing: false,
    }]);
    let (prog, _, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("r", 2);
    assert_eq!(rows.len(), 1, "self index still loads alone: {rows:?}");
    assert_eq!(rows[0][1], "app/main.go");
}
