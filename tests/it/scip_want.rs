//! Lazy multi-repo SCIP via the user-derived `scip_want(repo)` demand rel: the
//! self index plus each wanted repo's index merge into ONE load, so a
//! cross-repo reference resolves its def_file (the per-index load could not).
//! Drives the engine in-proc over two ticks (want derives on tick 1, the merged
//! load lands on tick 2 — the data-driven-scan latency contract).

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

/// The shared symbol: defined in the dep repo's index, referenced (with no
/// local def) in the self repo's index.
const SYM: &str = "scip-go gomod lib 1.0.0 `lib`/Answer().";

#[test]
fn scip_want_merges_wanted_repo_indexes() {
    let d = sandbox("merge");
    let dep = d.join("dep-repo");
    fs::create_dir_all(&dep).unwrap();
    // self index: app/main.go REFERENCES the dep symbol, no def anywhere here.
    write_index(&d.join("index.scip"),
        vec![document("app/main.go", vec![occurrence(SYM, 0)])]);
    // dep index: lib/lib.go DEFINES it.
    write_index(&dep.join("index.scip"),
        vec![document("lib/lib.go", vec![occurrence(SYM, SymbolRole::Definition as i32)])]);

    fs::write(d.join("p.dl"),
        "rel scip_want(repo: text).\n\
         scip_want(\"dep\").\n\
         rel r(file: text, symbol: text, def_file: text).\n\
         r(F, S, D) <- scip_ref(F, S, D).\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![RepoConfig {
        slug: "dep".into(), root: dep.clone(), url: None, allow_missing: false,
    }]);
    let (prog, diags, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    let errs = diags.iter().filter(|x| x.severity == sprefa_v5::ast::Severity::Error).count();
    assert_eq!(errs, 0, "scip_want program should typecheck: {diags:?}");

    // Tick 1: want is empty at refresh time — only the self index loads, and a
    // ref whose symbol has no def in the loaded index is dropped entirely
    // (scip_import keeps resolved refs only), so `r` stays empty.
    eng.tick(&prog, true).unwrap();
    let rows = eng.rel_rows("r", 3);
    assert!(rows.is_empty(),
        "tick 1 must not already resolve across repos: {rows:?}");

    // Tick 2: the want row demands dep; merged load resolves the def_file.
    eng.tick(&prog, true).unwrap();
    let rows = eng.rel_rows("r", 3);
    let resolved = rows.iter().find(|r| r[0] == "app/main.go")
        .unwrap_or_else(|| panic!("no self-repo ref row on tick 2: {rows:?}"));
    assert_eq!(resolved[1], SYM);
    assert_eq!(resolved[2], "lib/lib.go",
        "merged index should resolve the cross-repo def: {rows:?}");
    // the dep repo's own def row rides along
    assert!(rows.iter().any(|r| r[0] == "lib/lib.go" || r[2] == "lib/lib.go"),
        "dep documents should be in the merged load: {rows:?}");
}

/// A wanted repo with no index and no installed indexer for its (absent)
/// markers skips loudly; the self index still loads alone.
#[test]
fn scip_want_skips_unindexable_repo() {
    let d = sandbox("skip");
    let dep = d.join("bare-repo");
    fs::create_dir_all(&dep).unwrap(); // no markers, no index
    write_index(&d.join("index.scip"),
        vec![document("app/main.go", vec![occurrence(SYM, SymbolRole::Definition as i32)])]);

    fs::write(d.join("p.dl"),
        "rel scip_want(repo: text).\n\
         scip_want(\"bare\").\n\
         scip_want(\"no-such-slug\").\n\
         rel r(symbol: text, file: text).\n\
         r(S, F) <- scip_def(S, F).\n").unwrap();
    let conn = db::open(Some(d.join("db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, d.clone());
    eng.set_repos(vec![RepoConfig {
        slug: "bare".into(), root: dep.clone(), url: None, allow_missing: false,
    }]);
    let (prog, _, _) = prepare_paths(&[d.join("p.dl")]).unwrap();
    eng.tick(&prog, true).unwrap();
    eng.tick(&prog, true).unwrap();

    let rows = eng.rel_rows("r", 2);
    assert_eq!(rows.len(), 1, "self index still loads alone: {rows:?}");
    assert_eq!(rows[0][1], "app/main.go");
}
