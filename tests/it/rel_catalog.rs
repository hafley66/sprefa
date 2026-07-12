//! Self-describing built-in catalog (`rel_catalog`) + the doc-completeness
//! invariant. Every engine-emitted relation must carry a one-line doc on its
//! `RelDecl` (`group`/`doc` fields); `undocumented_builtins()` is the guard, and
//! this test fails the build the moment a new built-in ships without a doc — so
//! the documentation standard is mechanical, not a review checklist. The
//! generated README table (examples/builtin-rels.dl) reads `rel_catalog`, so it
//! can never drift from the engine's actual declarations.

use sprefa_v5::{db, engine::{self, Engine}, lex, parse};
use std::path::PathBuf;

fn s(v: &serde_json::Value) -> String { v.as_str().unwrap_or_default().to_string() }

#[test]
fn every_builtin_relation_is_documented() {
    let missing = engine::undocumented_builtins();
    assert!(missing.is_empty(),
        "every built-in relation needs group/doc set on its RelDecl; \
         undocumented: {missing:?}");
}

#[test]
fn every_scalar_function_is_documented() {
    let missing = engine::undocumented_fns();
    assert!(missing.is_empty(),
        "every scalar function (STR_FNS + split/replace/int) needs a (name, arity, group, doc) \
         row in fn_docs; undocumented: {missing:?}");
}

#[test]
fn fn_catalog_is_self_describing_and_complete() {
    let root = std::env::temp_dir().join(format!("fn_catalog_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let prog = parse::parse(lex::lex("? fn_catalog(name, arity, group, doc).").unwrap()).unwrap();
    let conn = db::open(Some(root.join("c.db").to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root);
    eng.tick(&prog, true).unwrap();

    let rows = eng.query_sql(
        "SELECT \"name\",\"arity\",\"group\",\"doc\" FROM rel_fn_catalog_txt", &[]).unwrap();
    assert!(rows.iter().any(|r| s(&r[0]) == "split"), "fn_catalog lists split");
    assert!(rows.iter().any(|r| s(&r[0]) == "replace_re"), "fn_catalog lists the new replace_re");
    for r in &rows {
        let (name, doc) = (s(&r[0]), s(&r[3]));
        assert!(!doc.is_empty(), "{name} has no doc");
        assert!(!doc.contains('|'), "{name} doc has a pipe, would break the md table: {doc}");
    }
}

#[test]
fn rel_catalog_is_self_describing_and_complete() {
    let root = std::env::temp_dir().join(format!("rel_catalog_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    let prog = parse::parse(lex::lex("? rel_catalog(name, group, cols, doc).").unwrap()).unwrap();
    let dbp = root.join("c.db");
    let conn = db::open(Some(dbp.to_str().unwrap())).unwrap();
    let mut eng = Engine::new(conn, root);
    eng.tick(&prog, true).unwrap();

    let rows = eng.query_sql(
        "SELECT \"name\",\"group\",\"cols\",\"doc\" FROM rel_rel_catalog_txt", &[]).unwrap();

    // one row per declared built-in, none blank.
    assert_eq!(rows.len(), engine::builtin_rel_names().len(),
        "catalog must cover exactly the built-in set");
    for r in &rows {
        let (name, group, cols, doc) = (s(&r[0]), s(&r[1]), s(&r[2]), s(&r[3]));
        assert!(!group.is_empty(), "{name} has no group");
        assert!(!doc.is_empty(), "{name} has no doc");
        assert!(cols.starts_with('(') && cols.ends_with(')'), "{name} cols malformed: {cols}");
        assert!(!doc.contains('|'), "{name} doc has a pipe, would break the md table: {doc}");
    }

    // it describes ITSELF, and the dispatch primitive added this arc is present.
    let by_name = |n: &str| rows.iter().find(|r| s(&r[0]) == n).map(|r| (s(&r[1]), s(&r[2]), s(&r[3])));
    let cat = by_name("rel_catalog").expect("catalog lists itself");
    assert_eq!(cat.0, "meta", "catalog self-row group");
    let imp = by_name("scip_impl").expect("scip_impl documented");
    assert_eq!(imp.0, "scip");
    assert_eq!(imp.1, "(impl, iface)", "catalog cols come from the real decl");
    assert!(imp.2.contains("is_implementation"), "scip_impl doc: {}", imp.2);
}
