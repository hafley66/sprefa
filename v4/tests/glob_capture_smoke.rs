//! Glob capture syntax: ${X?} bind, ${X} read, $$$ multi-segment.
//! Uniform with re/ast/host carveouts.

use std::sync::Arc;
use effect_runtime::v2::{
    expand, ExpandOpts, FactStore, MemFactStore, MemQueue, QueueBackend,
};
use v4::Cursor;
use v4::compile::parse::host_parse;
use v4::compile::walk::walk_program;
use v4::lower::{default_registry, LowerCtx};

fn run_in(root: &std::path::Path, src: &str) -> Arc<dyn FactStore<Cursor>> {
    let (program, parse_diags) = host_parse(src);
    assert!(parse_diags.is_empty(), "parse: {:?}", parse_diags);
    let store: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let reg = default_registry();
    let mut ctx = LowerCtx::new(store.clone(), root.to_path_buf());
    let (pipes, walk_diags) = walk_program(&program, &reg, &mut ctx);
    assert!(walk_diags.is_empty(), "walk: {:?}",
        walk_diags.iter().map(|d| (d.code.as_ref(), d.message.as_str())).collect::<Vec<_>>());
    let queue: Arc<dyn QueueBackend<Cursor>> = Arc::new(MemQueue::new());
    for pipe in pipes {
        let inst = pipe.into_instance();
        expand(&inst, queue.clone(), vec![Arc::new(Cursor::default())], ExpandOpts::default());
    }
    store
}

/// ${X?} captures one path segment; the term binds into cursor.
#[test]
fn single_segment_capture_binds_term() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), b"x").unwrap();
    std::fs::write(tmp.path().join("bravo.rs"), b"y").unwrap();

    // For each cursor whose value matches the pattern, bind STEM and EXT.
    // Rule head uses `?` form to declare cols as binders (matches the
    // rule_decl_smoke convention: bareword + `?` = declared binder col).
    let store = run_in(tmp.path(),
        "rule(:bits, STEM?, EXT?) { fs > glob`${STEM?}.${EXT?}` };");
    let rows = store.rows_of("bits");
    assert_eq!(rows.len(), 2);

    // Each row's cursor should have STEM and EXT bound from the path.
    let mut bindings: Vec<(String, String)> = rows.iter().map(|c| {
        let stem = c.get("STEM").unwrap_or("").to_string();
        let ext = c.get("EXT").unwrap_or("").to_string();
        (stem, ext)
    }).collect();
    bindings.sort();
    assert!(bindings.iter().any(|(s, e)| s.ends_with("alpha") && e == "txt"),
        "expected (alpha, txt) in {:?}", bindings);
    assert!(bindings.iter().any(|(s, e)| s.ends_with("bravo") && e == "rs"),
        "expected (bravo, rs) in {:?}", bindings);
}

/// $$$ prefix on a capture = multi-segment greedy.
#[test]
fn multi_segment_capture_with_triple_dollar() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src/sub/deep")).unwrap();
    std::fs::write(tmp.path().join("src/sub/deep/leaf.rs"), b"x").unwrap();

    let store = run_in(tmp.path(),
        "rule(:tree, PARENT?, FILE?) { fs > glob`$$${PARENT?}/${FILE?}.rs` };");
    let rows = store.rows_of("tree");
    assert_eq!(rows.len(), 1);

    let parent = rows[0].get("PARENT").unwrap_or("").to_string();
    let file = rows[0].get("FILE").unwrap_or("").to_string();
    assert!(parent.ends_with("src/sub/deep"),
        "PARENT should be multi-segment, got {:?}", parent);
    assert_eq!(file, "leaf");
}

/// No-match → cursor filtered out (Node::Done).
#[test]
fn no_match_filters_cursor() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("just.txt"), b"x").unwrap();

    let store = run_in(tmp.path(),
        "rule(:nope) { fs > glob`${X?}.never` };");
    let rows = store.rows_of("nope");
    assert_eq!(rows.len(), 0, "no path matches *.never");
}

/// Captures-bound regression: existing v4_glob_smoke shape with
/// no captures still filters by glob pattern alone.
#[test]
fn pattern_only_no_captures_still_filters() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), b"x").unwrap();
    std::fs::write(tmp.path().join("b.txt"), b"y").unwrap();

    let store = run_in(tmp.path(),
        "rule(:rs) { fs > glob`**/*.rs` };");
    let rows = store.rows_of("rs");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].value.ends_with(".rs"));
}
