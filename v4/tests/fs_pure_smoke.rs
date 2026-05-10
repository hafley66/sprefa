//! fs/glob/read substrate cleanup. fs = pure path query. glob = pure
//! text pattern over cursor.value. read = effect that turns the path
//! in cursor.value into bytes.

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

/// fs is a pure query — emits one cursor per file under ctx.root.
/// cursor.value is the path string. No content yet.
#[test]
fn fs_emits_cursors_with_value_equal_to_path() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"alpha").unwrap();
    std::fs::write(tmp.path().join("b.txt"), b"bravo").unwrap();

    let store = run_in(tmp.path(), "rule(:paths) { fs };");
    let rows = store.rows_of("paths");
    assert_eq!(rows.len(), 2, "expected one cursor per file");

    let mut values: Vec<String> = rows.iter().map(|c| c.value.to_string()).collect();
    values.sort();
    assert!(values[0].ends_with("a.txt"), "got {:?}", values);
    assert!(values[1].ends_with("b.txt"), "got {:?}", values);
    // cursor.value contains a path (not file content).
    for v in &values {
        assert!(!v.contains("alpha"), "fs should not have read content: {:?}", v);
        assert!(!v.contains("bravo"), "fs should not have read content: {:?}", v);
    }
}

/// glob is pure — operates on cursor.value as text haystack. Filters
/// to cursors whose value matches the glob pattern. No filesystem.
#[test]
fn glob_filters_cursor_value_no_filesystem() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"x").unwrap();
    std::fs::write(tmp.path().join("b.rs"),  b"y").unwrap();
    std::fs::write(tmp.path().join("c.rs"),  b"z").unwrap();

    // fs emits 3 cursors with value=path; glob filters to .rs only.
    let store = run_in(tmp.path(), "rule(:rs) { fs > glob`**/*.rs` };");
    let rows = store.rows_of("rs");
    assert_eq!(rows.len(), 2, "expected b.rs + c.rs only");
    for c in rows.iter() {
        assert!(c.value.ends_with(".rs"), "got {:?}", &*c.value);
    }
}

/// read is the effect — reads cursor.value as a path, flips value to
/// the loaded bytes.
#[test]
fn read_loads_bytes_from_cursor_value_path() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("only.txt"), b"loaded-bytes").unwrap();

    let store = run_in(tmp.path(), "rule(:bodies) { fs > read };");
    let rows = store.rows_of("bodies");
    assert_eq!(rows.len(), 1);
    assert_eq!(&*rows[0].value, "loaded-bytes",
        "read should flip cursor.value from path to file content");
}

/// Composition: fs > glob > read. glob filters by path, read loads.
#[test]
fn fs_glob_read_chain() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.txt"), b"alpha").unwrap();
    std::fs::write(tmp.path().join("b.rs"),  b"bravo").unwrap();

    let store = run_in(tmp.path(), "rule(:rs_bodies) { fs > glob`**/*.rs` > read };");
    let rows = store.rows_of("rs_bodies");
    assert_eq!(rows.len(), 1, "only b.rs survives the glob filter");
    assert_eq!(&*rows[0].value, "bravo", "read loaded b.rs content");
}

/// Source-side filter form: fs(glob`...`) keeps the Sprf surface small
/// while letting fs reject candidate paths before emitting them into the
/// runtime queue.
#[test]
fn fs_accepts_glob_filter_arg() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), b"lib").unwrap();
    std::fs::write(tmp.path().join("src/main.rs"), b"main").unwrap();
    std::fs::write(tmp.path().join("README.md"), b"readme").unwrap();

    let store = run_in(tmp.path(), "rule(:rs_paths) { fs(glob`**/*.rs`) };");
    let rows = store.rows_of("rs_paths");
    let mut values: Vec<String> = rows.iter().map(|c| c.value.to_string()).collect();
    values.sort();

    assert_eq!(values.len(), 2, "expected only Rust source files: {values:?}");
    assert!(values[0].ends_with("src/lib.rs"), "got {values:?}");
    assert!(values[1].ends_with("src/main.rs"), "got {values:?}");
}
